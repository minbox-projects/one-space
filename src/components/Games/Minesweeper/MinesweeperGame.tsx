import { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { ArrowLeft, Volume2, VolumeX, Flag, Bomb, Timer, Play } from 'lucide-react';
import { MinesweeperEngine } from './MinesweeperEngine';
import type { MinesweeperBoard, Cell } from './MinesweeperEngine';
import { minesweeperAudio } from './MinesweeperAudio';

// --- Types ---
type Difficulty = 'beginner' | 'intermediate' | 'expert';
type GameState = 'IDLE' | 'PLAYING' | 'WON' | 'LOST';

interface DifficultyConfig {
  rows: number;
  cols: number;
  mines: number;
}

const DIFFICULTIES: Record<Difficulty, DifficultyConfig> = {
  beginner: { rows: 9, cols: 9, mines: 10 },
  intermediate: { rows: 16, cols: 16, mines: 40 },
  expert: { rows: 16, cols: 30, mines: 99 },
};

interface GameData {
  minesweeper_best_times?: Record<Difficulty, number>;
}

export const MinesweeperGame = ({ onBack }: { onBack: () => void }) => {
  const { t } = useTranslation();
  const isTauri = '__TAURI_INTERNALS__' in window;

  // --- State ---
  const [gameState, setGameState] = useState<GameState>('IDLE');
  const [difficulty, setDifficulty] = useState<Difficulty>('beginner');
  const [board, setBoard] = useState<MinesweeperBoard>([]);
  const [minesLeft, setMines] = useState(0);
  const [timer, setTimer] = useState(0);
  const [bestTimes, setBestTimes] = useState<Record<Difficulty, number>>({
    beginner: 0,
    intermediate: 0,
    expert: 0,
  });
  const [isMuted, setIsMuted] = useState(false);

  // --- Refs ---
  const isFirstClick = useRef(true);
  const timerInterval = useRef<ReturnType<typeof setInterval> | null>(null);

  // --- Persistence ---
  const loadBestTimes = useCallback(async () => {
    if (!isTauri) return;
    try {
      const dataStr = await invoke<string>('read_game_data');
      const data = JSON.parse(dataStr) as GameData;
      if (data?.minesweeper_best_times) {
        setBestTimes(data.minesweeper_best_times);
      }
    } catch (e) {
      console.error('Failed to load best times', e);
    }
  }, [isTauri]);

  const saveBestTime = useCallback(async (time: number) => {
    if (!isTauri) return;
    const currentBest = bestTimes[difficulty];
    if (currentBest > 0 && time >= currentBest) return;

    try {
      const dataStr = await invoke<string>('read_game_data');
      let data: any = {};
      try { data = JSON.parse(dataStr); } catch {}
      if (Array.isArray(data)) data = {};

      if (!data.minesweeper_best_times) data.minesweeper_best_times = {};
      data.minesweeper_best_times[difficulty] = time;
      
      await invoke('save_game_data', { dataJson: JSON.stringify(data) });
      setBestTimes(prev => ({ ...prev, [difficulty]: time }));
    } catch (e) {
      console.error('Failed to save best time', e);
    }
  }, [difficulty, bestTimes, isTauri]);

  // --- Game Logic ---
  const initGame = useCallback((diff: Difficulty) => {
    const config = DIFFICULTIES[diff];
    setDifficulty(diff);
    setBoard(MinesweeperEngine.createBoard(config.rows, config.cols));
    setMines(config.mines);
    setTimer(0);
    setGameState('PLAYING');
    isFirstClick.current = true;
    if (timerInterval.current) clearInterval(timerInterval.current);
  }, []);

  const handleCellClick = (r: number, c: number) => {
    if (gameState !== 'PLAYING' || board[r][c].isFlagged || board[r][c].isRevealed) return;

    let currentBoard = board;
    const config = DIFFICULTIES[difficulty];

    if (isFirstClick.current) {
      currentBoard = MinesweeperEngine.plantMines(board, config.mines, r, c);
      isFirstClick.current = false;
      timerInterval.current = setInterval(() => setTimer(t => t + 1), 1000);
    }

    if (currentBoard[r][c].value === 'mine') {
      // Game Over
      const newBoard = currentBoard.map(row =>
        row.map(cell => ({
          ...cell,
          isRevealed: cell.value === 'mine' ? true : cell.isRevealed,
        }))
      );
      newBoard[r][c].isExploded = true;
      setBoard(newBoard);
      setGameState('LOST');
      minesweeperAudio.playExplosion();
      if (timerInterval.current) clearInterval(timerInterval.current);
    } else {
      const newBoard = MinesweeperEngine.revealCell(currentBoard, r, c);
      setBoard(newBoard);
      minesweeperAudio.playClick();

      if (MinesweeperEngine.checkWin(newBoard, config.mines)) {
        setGameState('WON');
        minesweeperAudio.playWin();
        if (timerInterval.current) clearInterval(timerInterval.current);
        saveBestTime(timer);
      }
    }
  };

  const handleRightClick = (e: React.MouseEvent, r: number, c: number) => {
    e.preventDefault();
    if (gameState !== 'PLAYING' || board[r][c].isRevealed) return;

    const newBoard = MinesweeperEngine.toggleFlag(board, r, c);
    setBoard(newBoard);
    setMines(prev => (board[r][c].isFlagged ? prev + 1 : prev - 1));
    minesweeperAudio.playFlag();
  };

  // --- Effects ---
  useEffect(() => {
    loadBestTimes();
    return () => {
      if (timerInterval.current) clearInterval(timerInterval.current);
    };
  }, [loadBestTimes]);

  useEffect(() => {
    minesweeperAudio.setMuted(isMuted);
  }, [isMuted]);

  // --- Render Helpers ---
  const getCellContent = (cell: Cell) => {
    if (cell.isFlagged && !cell.isRevealed) return <Flag className="w-4 h-4 text-primary" />;
    if (!cell.isRevealed) return null;
    if (cell.value === 'mine') return <Bomb className={`w-4 h-4 ${cell.isExploded ? 'text-white' : 'text-foreground'}`} />;
    if (cell.value === 'empty') return null;
    return cell.value;
  };

  const getNumberColor = (val: number) => {
    const colors = [
      '',
      'text-blue-500',
      'text-green-500',
      'text-red-500',
      'text-purple-500',
      'text-amber-500',
      'text-cyan-500',
      'text-pink-500',
      'text-gray-500',
    ];
    return colors[val] || '';
  };

  if (gameState === 'IDLE') {
    return (
      <div className="h-full flex flex-col bg-background select-none">
        <header className="flex items-center justify-between p-4 border-b bg-background/80 backdrop-blur z-10">
          <div className="flex items-center gap-4">
            <button onClick={onBack} className="p-2 hover:bg-muted rounded-full transition-colors">
              <ArrowLeft className="w-5 h-5" />
            </button>
            <div>
              <h2 className="text-lg font-bold">{t('minesweeper', 'Deep Minesweeper')}</h2>
              <p className="text-xs text-muted-foreground">{t('minesweeperDesc', 'Classic strategy game. Clear the field without hitting a mine.')}</p>
            </div>
          </div>
        </header>
        <main className="flex-1 flex flex-col items-center justify-center p-8 text-center">
          <div className="max-w-md w-full">
            <h1 className="text-4xl font-black tracking-tighter mb-8 italic">DEEP MINESWEEPER</h1>
            <div className="grid grid-cols-1 gap-4">
              {(Object.keys(DIFFICULTIES) as Difficulty[]).map(d => (
                <button
                  key={d}
                  onClick={() => initGame(d)}
                  className="group relative flex items-center justify-between p-6 rounded-2xl border bg-card hover:border-primary hover:shadow-xl transition-all"
                >
                  <div className="text-left">
                    <div className="font-bold uppercase tracking-widest text-sm">{t(`minesweeperDiff_${d}`, d)}</div>
                    <div className="text-xs text-muted-foreground mt-1">
                      {DIFFICULTIES[d].rows}x{DIFFICULTIES[d].cols} • {DIFFICULTIES[d].mines} {t('mines', 'Mines')}
                    </div>
                  </div>
                  {bestTimes[d] > 0 && (
                    <div className="text-right">
                      <div className="text-[10px] text-muted-foreground uppercase">{t('bestTime', 'Best')}</div>
                      <div className="font-mono font-bold text-primary">{bestTimes[d]}s</div>
                    </div>
                  )}
                  <Play className="w-5 h-5 text-primary opacity-0 group-hover:opacity-100 transition-opacity" />
                </button>
              ))}
            </div>
          </div>
        </main>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-background select-none overflow-hidden relative">
      <header className="flex items-center justify-between p-4 border-b bg-background/80 backdrop-blur z-10">
        <div className="flex items-center gap-4">
          <button onClick={() => setGameState('IDLE')} className="p-2 hover:bg-muted rounded-full transition-colors">
            <ArrowLeft className="w-5 h-5" />
          </button>
          <div>
            <h2 className="text-lg font-bold">{t('minesweeper', 'Minesweeper')}</h2>
            <div className="flex items-center gap-3 text-xs text-muted-foreground">
              <span className="uppercase font-bold text-primary">{t(`minesweeperDiff_${difficulty}`, difficulty)}</span>
            </div>
          </div>
        </div>

        <div className="flex items-center gap-6">
          <div className="flex items-center gap-2 bg-muted/50 px-3 py-1.5 rounded-lg">
            <Bomb className="w-4 h-4 text-muted-foreground" />
            <span className="font-mono font-bold text-lg">{minesLeft}</span>
          </div>
          <div className="flex items-center gap-2 bg-muted/50 px-3 py-1.5 rounded-lg">
            <Timer className="w-4 h-4 text-muted-foreground" />
            <span className="font-mono font-bold text-lg">{timer}s</span>
          </div>
          <button onClick={() => setIsMuted(!isMuted)} className="p-2 hover:bg-muted rounded-full transition-colors">
            {isMuted ? <VolumeX className="w-5 h-5" /> : <Volume2 className="w-5 h-5" />}
          </button>
        </div>
      </header>

      <main className="flex-1 overflow-auto p-8 flex justify-center items-start bg-muted/5">
        <div 
          className="grid gap-[1px] bg-foreground/10 border-2 border-foreground/20 shadow-2xl rounded-sm p-[1px] h-fit"
          style={{
            gridTemplateColumns: `repeat(${DIFFICULTIES[difficulty].cols}, minmax(32px, 32px))`,
          }}
        >
          {board.map((row, r) =>
            row.map((cell, c) => (
              <div
                key={`${r}-${c}`}
                onClick={() => handleCellClick(r, c)}
                onContextMenu={(e) => handleRightClick(e, r, c)}
                className={`
                  w-8 h-8 flex items-center justify-center text-sm font-bold cursor-pointer transition-all duration-75
                  rounded-sm select-none
                  ${cell.isRevealed 
                    ? cell.isExploded ? 'bg-destructive shadow-inner' : 'bg-muted/30 shadow-inner' 
                    : 'bg-card shadow-[inset_-2px_-2px_0_rgba(0,0,0,0.1),inset_2px_2px_0_rgba(255,255,255,0.1)] hover:bg-muted hover:shadow-none'}
                  ${typeof cell.value === 'number' ? getNumberColor(cell.value) : ''}
                `}
              >
                {getCellContent(cell)}
              </div>
            ))
          )}
        </div>
      </main>

      {/* Overlays */}
      {(gameState === 'WON' || gameState === 'LOST') && (
        <div className="absolute inset-0 bg-background/80 backdrop-blur-md z-50 flex flex-col items-center justify-center p-8 text-center animate-in zoom-in duration-300">
          <div className={`w-20 h-20 rounded-full flex items-center justify-center mb-6 ${gameState === 'WON' ? 'bg-primary/10 text-primary' : 'bg-destructive/10 text-destructive'}`}>
            {gameState === 'WON' ? <Play className="w-10 h-10 rotate-[-90deg]" /> : <Bomb className="w-10 h-10" />}
          </div>
          <h1 className="text-5xl font-black tracking-tighter mb-2 italic">
            {gameState === 'WON' ? 'VICTORY!' : 'BOOM!'}
          </h1>
          <p className="text-muted-foreground mb-8">
            {gameState === 'WON' 
              ? t('minesweeperWin', 'Field cleared safely. Excellent job!') 
              : t('minesweeperLose', 'You triggered a mine. Try again.')}
          </p>
          
          <div className="grid grid-cols-2 gap-8 mb-12">
            <div className="text-center">
              <div className="text-xs text-muted-foreground uppercase tracking-widest mb-1">{t('sudokuTimeTaken', 'Time')}</div>
              <div className="text-3xl font-mono font-bold">{timer}s</div>
            </div>
            {gameState === 'WON' && bestTimes[difficulty] > 0 && (
              <div className="text-center border-l pl-8">
                <div className="text-xs text-muted-foreground uppercase tracking-widest mb-1">{t('bestTime', 'Best')}</div>
                <div className="text-3xl font-mono font-bold text-primary">{bestTimes[difficulty]}s</div>
              </div>
            )}
          </div>

          <div className="flex gap-4">
            <button onClick={() => setGameState('IDLE')} className="px-8 py-3 rounded-full border bg-card hover:bg-muted font-bold transition-all">
              {t('backToMenu', 'MENU')}
            </button>
            <button onClick={() => initGame(difficulty)} className="px-10 py-3 rounded-full bg-primary text-primary-foreground font-bold hover:scale-105 transition-all shadow-xl shadow-primary/20">
              {t('playAgain', 'RETRY')}
            </button>
          </div>
        </div>
      )}
    </div>
  );
};

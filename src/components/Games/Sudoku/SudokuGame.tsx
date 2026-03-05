import { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { ArrowLeft, Volume2, VolumeX, RotateCcw, Pencil, Eraser, Lightbulb, Undo2 } from 'lucide-react';
import { useTheme } from '../../ThemeProvider';
import { SudokuEngine } from './SudokuEngine';
import type { SudokuBoard } from './SudokuEngine';
import { sudokuAudio } from './SudokuAudio';

// --- Types ---
type Difficulty = 'easy' | 'medium' | 'hard' | 'expert';
type GameState = 'IDLE' | 'PLAYING' | 'WON';

interface GameData {
    sudoku_current?: {
        difficulty: Difficulty;
        board: SudokuBoard;
        solution: number[][];
        initial: SudokuBoard;
        notes: number[][][];
        timer: number;
        mistakes: number;
    };
    sudoku_stats?: {
        won_count: Record<Difficulty, number>;
        best_times: Record<Difficulty, number>;
    };
}

export const SudokuGame = ({ onBack }: { onBack: () => void }) => {
    const { t } = useTranslation();
    const { theme } = useTheme();
    const isTauri = '__TAURI_INTERNALS__' in window;

    // --- State ---
    const [gameState, setGameState] = useState<GameState>('IDLE');
    const [difficulty, setDifficulty] = useState<Difficulty>('medium');
    const [board, setBoard] = useState<SudokuBoard>(Array(9).fill(null).map(() => Array(9).fill(null)));
    const [solution, setSolution] = useState<number[][]>([]);
    const [initial, setInitial] = useState<SudokuBoard>([]);
    const [notes, setNotes] = useState<number[][][]>(Array(9).fill(null).map(() => Array(9).fill(null).map(() => [])));
    const [selected, setSelected] = useState<{ r: number, c: number } | null>(null);
    const [mistakes, setMistakes] = useState(0);
    const [timer, setTimer] = useState(0);
    const [isNoteMode, setIsNoteMode] = useState(false);
    const [isMuted, setIsMuted] = useState(false);
    const [history, setHistory] = useState<SudokuBoard[]>([]);

    // --- Refs ---
    const timerInterval = useRef<any>(null);
    const initialized = useRef(false);

    // --- Persistence ---
    const clearProgress = useCallback(async () => {
        if (!isTauri) {
            localStorage.removeItem('sudoku_current');
            return;
        }
        try {
            const dataStr = await invoke<string>('read_game_data');
            let data: any = {};
            try { data = JSON.parse(dataStr); } catch {}
            if (Array.isArray(data)) data = {};
            delete data.sudoku_current;
            await invoke('save_game_data', { dataJson: JSON.stringify(data) });
        } catch (e) { console.error('Failed to clear Sudoku progress', e); }
    }, [isTauri]);

    const saveProgress = useCallback(async (currentBoard: SudokuBoard, currentNotes: number[][][], currentTimer: number, currentMistakes: number) => {
        if (!isTauri || gameState !== 'PLAYING') return;
        try {
            const dataStr = await invoke<string>('read_game_data');
            let data: any = {};
            try { data = JSON.parse(dataStr); } catch {}
            if (Array.isArray(data)) data = {};

            data.sudoku_current = {
                difficulty,
                board: currentBoard,
                solution,
                initial,
                notes: currentNotes,
                timer: currentTimer,
                mistakes: currentMistakes
            };
            await invoke('save_game_data', { dataJson: JSON.stringify(data) });
        } catch (e) { console.error('Failed to save Sudoku progress', e); }
    }, [difficulty, solution, initial, isTauri, gameState]);

    const loadProgress = useCallback(async () => {
        if (!isTauri) return false;
        try {
            const dataStr = await invoke<string>('read_game_data');
            const data = JSON.parse(dataStr) as GameData;
            if (data?.sudoku_current) {
                const cur = data.sudoku_current;
                setDifficulty(cur.difficulty);
                setBoard(cur.board);
                setSolution(cur.solution);
                setInitial(cur.initial);
                setNotes(cur.notes);
                setTimer(cur.timer);
                setMistakes(cur.mistakes);
                setGameState('PLAYING');
                return true;
            }
        } catch (e) { console.error('Failed to load Sudoku progress', e); }
        return false;
    }, [isTauri]);

    // --- Game Logic ---
    const startNewGame = useCallback((diff: Difficulty) => {
        const puzzle = SudokuEngine.generatePuzzle(diff);
        setDifficulty(diff);
        setBoard(puzzle.initial.map(row => [...row]));
        setInitial(puzzle.initial.map(row => [...row]));
        setSolution(puzzle.solution);
        setNotes(Array(9).fill(null).map(() => Array(9).fill(null).map(() => [])));
        setMistakes(0);
        setTimer(0);
        setHistory([]);
        setGameState('PLAYING');
        setSelected({ r: 4, c: 4 });
        sudokuAudio.playInput();
    }, []);

    const handleCellInput = useCallback((num: number | null) => {
        if (gameState !== 'PLAYING' || !selected) return;
        const { r, c } = selected;
        if (initial[r][c] !== null) return; // Cannot edit initial cells

        if (isNoteMode && num !== null) {
            const newNotes = [...notes];
            const cellNotes = [...newNotes[r][c]];
            if (cellNotes.includes(num)) {
                newNotes[r][c] = cellNotes.filter(n => n !== num);
            } else {
                newNotes[r][c] = [...cellNotes, num].sort();
            }
            setNotes(newNotes);
            sudokuAudio.playNote();
            return;
        }

        // Standard input
        if (board[r][c] === num) return;

        const newBoard = board.map(row => [...row]);
        newBoard[r][c] = num;

        if (num !== null && num !== solution[r][c]) {
            setMistakes(m => m + 1);
            sudokuAudio.playError();
        } else {
            sudokuAudio.playInput();
        }

        setHistory(prev => [...prev.slice(-19), board.map(row => [...row])]);
        setBoard(newBoard);

        // Check for win
        if (num !== null && num === solution[r][c]) {
            const isComplete = newBoard.every((row, ri) => 
                row.every((val, ci) => val === solution[ri][ci])
            );
            if (isComplete) {
                setGameState('WON');
                clearProgress();
                sudokuAudio.playWin();
            }
        }
    }, [gameState, selected, initial, board, isNoteMode, notes, solution, clearProgress]);

    const undo = () => {
        if (history.length === 0) return;
        const prev = history[history.length - 1];
        setBoard(prev);
        setHistory(history.slice(0, -1));
        sudokuAudio.playInput();
    };

    const handleBackToMenu = useCallback(() => {
        setGameState('IDLE');
        clearProgress();
    }, [clearProgress]);

    const hint = () => {
        if (!selected || gameState !== 'PLAYING') return;
        const { r, c } = selected;
        if (board[r][c] === solution[r][c]) return;
        handleCellInput(solution[r][c]);
    };

    // --- Effects ---
    useEffect(() => {
        if (!initialized.current) {
            loadProgress().then(success => {
                if (!success) setGameState('IDLE');
            });
            initialized.current = true;
        }
    }, [loadProgress]);

    useEffect(() => {
        if (gameState === 'PLAYING') {
            timerInterval.current = setInterval(() => setTimer(t => t + 1), 1000);
        } else {
            clearInterval(timerInterval.current);
        }
        return () => clearInterval(timerInterval.current);
    }, [gameState]);

    // Auto-save
    useEffect(() => {
        if (initialized.current && gameState === 'PLAYING') {
            saveProgress(board, notes, timer, mistakes);
        }
    }, [board, notes, timer, mistakes, gameState, saveProgress]);

    useEffect(() => {
        const handleKeyDown = (e: KeyboardEvent) => {
            if (gameState !== 'PLAYING') return;
            
            if (e.key >= '1' && e.key <= '9') {
                handleCellInput(parseInt(e.key));
            } else if (e.key === 'Backspace' || e.key === 'Delete') {
                handleCellInput(null);
            } else if (e.key === 'n' || e.key === 'N') {
                setIsNoteMode(prev => !prev);
            } else if (e.key === 'u' || e.key === 'U') {
                undo();
            } else if (selected) {
                const { r, c } = selected;
                if (e.key === 'ArrowUp' || e.key === 'w') setSelected({ r: Math.max(0, r - 1), c });
                if (e.key === 'ArrowDown' || e.key === 's') setSelected({ r: Math.min(8, r + 1), c });
                if (e.key === 'ArrowLeft' || e.key === 'a') setSelected({ r, c: Math.max(0, c - 1) });
                if (e.key === 'ArrowRight' || e.key === 'd') setSelected({ r, c: Math.min(8, c + 1) });
            }
        };
        window.addEventListener('keydown', handleKeyDown);
        return () => window.removeEventListener('keydown', handleKeyDown);
    }, [gameState, selected, handleCellInput]);

    useEffect(() => { sudokuAudio.setMuted(isMuted); }, [isMuted]);

    const formatTime = (s: number) => {
        const mins = Math.floor(s / 60);
        const secs = s % 60;
        return `${mins}:${secs.toString().padStart(2, '0')}`;
    };

    // --- Render Helpers ---
    const getCellClass = (r: number, c: number) => {
        const isSelected = selected?.r === r && selected?.c === c;
        const isInitial = initial[r][c] !== null;
        const isError = board[r][c] !== null && board[r][c] !== solution[r][c];
        const isRelated = selected && (selected.r === r || selected.c === c || (Math.floor(selected.r/3) === Math.floor(r/3) && Math.floor(selected.c/3) === Math.floor(c/3)));
        const isSameNum = selected && board[selected.r][selected.c] !== null && board[selected.r][selected.c] === board[r][c];

        return `
            relative flex items-center justify-center text-2xl font-medium cursor-pointer transition-all duration-150
            border-[0.5px] border-border/30
            ${c % 3 === 2 && c !== 8 ? 'border-r-2 border-r-foreground/30' : ''}
            ${r % 3 === 2 && r !== 8 ? 'border-b-2 border-b-foreground/30' : ''}
            ${isSelected ? 'bg-primary text-primary-foreground z-10 scale-105 shadow-lg rounded-sm' : 
              isSameNum ? 'bg-primary/30' :
              isRelated ? 'bg-primary/10' : 'hover:bg-muted/50'}
            ${isInitial ? 'font-bold' : 'text-primary'}
            ${isError ? 'text-destructive' : ''}
        `;
    };

    if (gameState === 'IDLE') {
        return (
            <div className="h-full flex flex-col bg-background select-none overflow-hidden relative">
                <header className="flex items-center justify-between p-4 border-b bg-background/80 backdrop-blur z-10">
                    <div className="flex items-center gap-4">
                        <button onClick={onBack} className="p-2 hover:bg-muted rounded-full transition-colors">
                            <ArrowLeft className="w-5 h-5" />
                        </button>
                        <div>
                            <h2 className="text-lg font-bold">{t('cyberSudoku', 'Zen Sudoku')}</h2>
                            <p className="text-xs text-muted-foreground">{t('sudokuDesc', 'Find peace in logic. Solve the classic number puzzle.')}</p>
                        </div>
                    </div>
                </header>
                
                <main className="flex-1 flex flex-col p-8 items-center justify-center text-center">
                    <div className="max-w-md w-full">
                        <h1 className="text-4xl font-black tracking-tighter mb-4 italic">{t('cyberSudoku', 'Zen Sudoku')}</h1>
                        <p className="text-muted-foreground mb-8">{t('sudokuWelcome', 'Find peace in logic. Choose a difficulty to begin.')}</p>
                        <div className="grid grid-cols-2 gap-4">
                            {(['easy', 'medium', 'hard', 'expert'] as Difficulty[]).map(d => (
                                <button key={d} onClick={() => startNewGame(d)} className="p-4 rounded-2xl border bg-card hover:border-primary hover:shadow-lg transition-all uppercase font-bold tracking-widest text-sm">
                                    {t(`sudokuDiff_${d}`, d)}
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
                    <button onClick={handleBackToMenu} className="p-2 hover:bg-muted rounded-full transition-colors">
                        <ArrowLeft className="w-5 h-5" />
                    </button>
                    <div>
                        <h2 className="text-lg font-bold">{t('cyberSudoku', 'Zen Sudoku')}</h2>
                        <div className="flex items-center gap-3 text-xs text-muted-foreground">
                            <span className="uppercase font-bold text-primary">{t(`sudokuDiff_${difficulty}`, difficulty)}</span>
                            <span>{formatTime(timer)}</span>
                            <span>{t('sudokuMistakes', 'Mistakes')}: {mistakes}</span>
                        </div>
                    </div>
                </div>
                <button onClick={() => setIsMuted(!isMuted)} className="p-2 hover:bg-muted rounded-full transition-colors">
                    {isMuted ? <VolumeX className="w-5 h-5" /> : <Volume2 className="w-5 h-5" />}
                </button>
            </header>

            <main className="flex-1 flex flex-col lg:flex-row items-center justify-center p-4 gap-8 overflow-y-auto">
                {/* 9x9 Grid */}
                <div className="aspect-square w-full max-w-[450px] grid grid-cols-9 grid-rows-9 border-2 border-foreground/40 bg-card shadow-2xl rounded-sm overflow-hidden">
                    {board.map((row, r) => row.map((cell, c) => (
                        <div key={`${r}-${c}`} className={getCellClass(r, c)} onClick={() => setSelected({ r, c })}>
                            {cell !== null ? cell : (
                                <div className="grid grid-cols-3 grid-rows-3 w-full h-full p-0.5 pointer-events-none">
                                    {[1,2,3,4,5,6,7,8,9].map(n => (
                                        <div key={n} className="flex items-center justify-center text-[8px] leading-none text-muted-foreground/60">
                                            {notes[r][c].includes(n) ? n : ''}
                                        </div>
                                    ))}
                                </div>
                            )}
                        </div>
                    )))}
                </div>

                {/* Controls */}
                <div className="flex flex-col gap-6 w-full max-w-[320px]">
                    <div className="grid grid-cols-3 gap-2">
                        {[1,2,3,4,5,6,7,8,9].map(n => (
                            <button key={n} onClick={() => handleCellInput(n)} className="h-14 rounded-xl border bg-card text-xl font-bold hover:bg-primary hover:text-primary-foreground transition-all shadow-sm">
                                {n}
                            </button>
                        ))}
                    </div>

                    <div className="grid grid-cols-4 gap-2">
                        <button onClick={undo} className="flex flex-col items-center justify-center py-2 rounded-xl border bg-card hover:bg-muted transition-all" title={t('sudokuUndo', 'Undo')}>
                            <Undo2 className="w-5 h-5 mb-1" />
                            <span className="text-[10px] uppercase font-bold">{t('sudokuUndo', 'Undo')}</span>
                        </button>
                        <button onClick={() => handleCellInput(null)} className="flex flex-col items-center justify-center py-2 rounded-xl border bg-card hover:bg-muted transition-all" title={t('sudokuErase', 'Erase')}>
                            <Eraser className="w-5 h-5 mb-1" />
                            <span className="text-[10px] uppercase font-bold">{t('sudokuErase', 'Erase')}</span>
                        </button>
                        <button 
                            onClick={() => setIsNoteMode(!isNoteMode)} 
                            className={`flex flex-col items-center justify-center py-2 rounded-xl border transition-all ${isNoteMode ? 'bg-primary text-primary-foreground border-primary shadow-lg shadow-primary/20' : 'bg-card hover:bg-muted'}`}
                            title={t('sudokuNotes', 'Notes')}
                        >
                            <Pencil className="w-5 h-5 mb-1" />
                            <span className="text-[10px] uppercase font-bold">{t('sudokuNotes', 'Notes')}</span>
                        </button>
                        <button onClick={hint} className="flex flex-col items-center justify-center py-2 rounded-xl border bg-card hover:bg-muted transition-all" title={t('sudokuHint', 'Hint')}>
                            <Lightbulb className="w-5 h-5 mb-1 text-amber-500" />
                            <span className="text-[10px] uppercase font-bold">{t('sudokuHint', 'Hint')}</span>
                        </button>
                    </div>

                    <button onClick={handleBackToMenu} className="w-full py-3 rounded-xl border bg-card hover:bg-muted font-bold tracking-widest text-xs flex items-center justify-center gap-2">
                        <RotateCcw className="w-4 h-4" /> {t('sudokuNewGame', 'NEW GAME')}
                    </button>
                </div>
            </main>

            {/* Win Overlay */}
            {gameState === 'WON' && (
                <div className="absolute inset-0 bg-background/90 backdrop-blur-md z-50 flex flex-col items-center justify-center p-8 text-center animate-in fade-in duration-500">
                    <div className="w-20 h-20 bg-primary/10 rounded-full flex items-center justify-center text-primary mb-6 animate-bounce">
                        <Undo2 className="w-10 h-10 rotate-180" />
                    </div>
                    <h1 className="text-5xl font-black tracking-tighter mb-2 italic">{t('excellent', 'EXCELLENT!')}</h1>
                    <p className="text-muted-foreground mb-8">{t('sudokuWinMsg', 'Your mind is clear and sharp.')}</p>
                    
                    <div className="grid grid-cols-2 gap-8 mb-12">
                        <div className="text-center">
                            <div className="text-xs text-muted-foreground uppercase tracking-widest mb-1">{t('sudokuTimeTaken', 'Time')}</div>
                            <div className="text-3xl font-mono font-bold">{formatTime(timer)}</div>
                        </div>
                        <div className="text-center border-l pl-8">
                            <div className="text-xs text-muted-foreground uppercase tracking-widest mb-1">{t('sudokuMistakes', 'Mistakes')}</div>
                            <div className="text-3xl font-mono font-bold">{mistakes}</div>
                        </div>
                    </div>

                    <div className="flex gap-4">
                        <button onClick={handleBackToMenu} className="px-8 py-3 rounded-full border bg-card hover:bg-muted font-bold transition-all">
                            {t('backToMenu', 'MENU')}
                        </button>
                        <button onClick={() => startNewGame(difficulty)} className="px-10 py-3 rounded-full bg-primary text-primary-foreground font-bold hover:scale-105 transition-all shadow-xl shadow-primary/20">
                            {t('playAgain', 'PLAY AGAIN')}
                        </button>
                    </div>
                </div>
            )}
        </div>
    );
};

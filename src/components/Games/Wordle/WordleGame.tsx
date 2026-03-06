import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { ArrowLeft, Volume2, VolumeX, RotateCcw, Share2, Trophy, HelpCircle, Delete, CornerDownLeft } from 'lucide-react';
import { wordleAudio } from './WordleAudio';
import { getDailyWord, getRandomWord } from './WordList';

// --- Types ---
type GameState = 'PLAYING' | 'WON' | 'LOST';
type Mode = 'DAILY' | 'PRACTICE';
type LetterStatus = 'correct' | 'present' | 'absent' | 'unused';

interface GameData {
  wordle_stats?: {
    games_played: number;
    win_rate: number;
    current_streak: number;
    max_streak: number;
    guess_distribution: number[];
  };
  wordle_daily?: {
    date: string;
    guesses: string[];
    completed: boolean;
  };
}

type WordleStats = NonNullable<GameData['wordle_stats']>;

const MAX_GUESSES = 6;
const WORD_LENGTH = 5;
const DEFAULT_STATS: WordleStats = {
  games_played: 0,
  win_rate: 0,
  current_streak: 0,
  max_streak: 0,
  guess_distribution: [0, 0, 0, 0, 0, 0],
};

export const WordleGame = ({ onBack }: { onBack: () => void }) => {
  const { t } = useTranslation();
  const isTauri = '__TAURI_INTERNALS__' in window;

  // --- State ---
  const [mode, setMode] = useState<Mode>('DAILY');
  const [gameState, setGameState] = useState<GameState>('PLAYING');
  const [targetWord, setTargetWord] = useState('');
  const [guesses, setGuesses] = useState<string[]>([]);
  const [currentGuess, setCurrentGuess] = useState('');
  const [isMuted, setIsMuted] = useState(false);
  const [showHelp, setShowHelp] = useState(false);
  const [shakeRow, setShakeRow] = useState<number | null>(null);
  const [stats, setStats] = useState<WordleStats>(DEFAULT_STATS);

  const initialized = useRef(false);

  // --- Helpers ---
  const getLetterStatus = (letter: string, index: number, word: string, target: string): LetterStatus => {
    if (target[index] === letter) return 'correct';
    if (target.includes(letter)) {
        // Handle double letters: only highlight 'present' if count allows
        const targetCount = target.split(letter).length - 1;
        const correctCount = word.split('').filter((l, i) => l === letter && target[i] === letter).length;
        const previousOccurrences = word.slice(0, index).split(letter).length - 1;
        if (previousOccurrences < targetCount - correctCount) return 'present';
    }
    return 'absent';
  };

  const getKeyboardStatus = useMemo(() => {
    const statusMap: Record<string, LetterStatus> = {};
    guesses.forEach(guess => {
      guess.split('').forEach((letter, i) => {
        const current = getLetterStatus(letter, i, guess, targetWord);
        if (statusMap[letter] === 'correct') return;
        if (statusMap[letter] === 'present' && current !== 'correct') return;
        statusMap[letter] = current;
      });
    });
    return statusMap;
  }, [guesses, targetWord]);

  // --- Persistence ---
  const saveStats = useCallback(async (isWin: boolean, guessCount: number) => {
    if (!isTauri) return;
    try {
      const dataStr = await invoke<string>('read_game_data');
      let data: any = {};
      try { data = JSON.parse(dataStr); } catch {}
      if (Array.isArray(data)) data = {};

      const currentStats: WordleStats = data.wordle_stats || { ...DEFAULT_STATS };

      currentStats.games_played += 1;
      if (isWin) {
        currentStats.current_streak += 1;
        currentStats.max_streak = Math.max(currentStats.max_streak, currentStats.current_streak);
        currentStats.guess_distribution[guessCount - 1] += 1;
      } else {
        currentStats.current_streak = 0;
      }
      
      const totalWins = currentStats.guess_distribution.reduce((a: number, b: number) => a + b, 0);
      currentStats.win_rate = Math.round((totalWins / currentStats.games_played) * 100);

      data.wordle_stats = currentStats;
      
      if (mode === 'DAILY') {
          data.wordle_daily = {
              date: new Date().toISOString().split('T')[0],
              guesses: [...guesses, currentGuess],
              completed: true
          };
      }

      await invoke('save_game_data', { dataJson: JSON.stringify(data) });
      setStats(currentStats);
    } catch (e) { console.error(e); }
  }, [isTauri, mode, guesses, currentGuess]);

  const loadData = useCallback(async () => {
    if (!isTauri) return;
    try {
      const dataStr = await invoke<string>('read_game_data');
      const data = JSON.parse(dataStr) as GameData;
      if (data.wordle_stats) setStats(data.wordle_stats);
      
      const today = new Date().toISOString().split('T')[0];
      if (data.wordle_daily && data.wordle_daily.date === today) {
          if (data.wordle_daily.completed) {
              setTargetWord(getDailyWord());
              setGuesses(data.wordle_daily.guesses);
              setGameState(data.wordle_daily.guesses.includes(getDailyWord()) ? 'WON' : 'LOST');
              setMode('DAILY');
              return true;
          }
      }
    } catch (e) { console.error(e); }
    return false;
  }, [isTauri]);

  // --- Game Logic ---
  const initGame = useCallback((newMode: Mode) => {
    setMode(newMode);
    const word = newMode === 'DAILY' ? getDailyWord() : getRandomWord();
    setTargetWord(word);
    setGuesses([]);
    setCurrentGuess('');
    setGameState('PLAYING');
    wordleAudio.playKey();
  }, []);

  const submitGuess = useCallback(() => {
    if (currentGuess.length !== WORD_LENGTH || gameState !== 'PLAYING') return;

    // TODO: Validate word exists in dictionary if needed
    
    const newGuesses = [...guesses, currentGuess];
    setGuesses(newGuesses);
    setCurrentGuess('');
    wordleAudio.playEnter();
    newGuesses.forEach((_, i) => wordleAudio.playReveal(i));

    if (currentGuess === targetWord) {
      setGameState('WON');
      wordleAudio.playSuccess();
      saveStats(true, newGuesses.length);
    } else if (newGuesses.length >= MAX_GUESSES) {
      setGameState('LOST');
      wordleAudio.playFail();
      saveStats(false, 0);
    }
  }, [currentGuess, guesses, gameState, targetWord, saveStats]);

  const onKeyPress = useCallback((key: string) => {
    if (gameState !== 'PLAYING') return;

    if (key === 'BACKSPACE') {
      setCurrentGuess(prev => prev.slice(0, -1));
      wordleAudio.playKey();
    } else if (key === 'ENTER') {
      if (currentGuess.length < WORD_LENGTH) {
          setShakeRow(guesses.length);
          setTimeout(() => setShakeRow(null), 500);
          return;
      }
      submitGuess();
    } else if (currentGuess.length < WORD_LENGTH && /^[A-Z]$/.test(key)) {
      setCurrentGuess(prev => prev + key);
      wordleAudio.playKey();
    }
  }, [currentGuess, gameState, guesses.length, submitGuess]);

  // --- Effects ---
  useEffect(() => {
    if (!initialized.current) {
      loadData().then(restored => {
          if (!restored) initGame('DAILY');
      });
      initialized.current = true;
    }
  }, [loadData, initGame]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const key = e.key.toUpperCase();
      if (key === 'BACKSPACE' || key === 'ENTER' || /^[A-Z]$/.test(key)) {
        onKeyPress(key);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onKeyPress]);

  useEffect(() => { wordleAudio.setMuted(isMuted); }, [isMuted]);

  // --- Share ---
  const shareResult = () => {
    const today = new Date().toISOString().split('T')[0];
    let text = `Cyber Wordle ${mode === 'DAILY' ? today : '(Practice)'} ${gameState === 'WON' ? guesses.length : 'X'}/${MAX_GUESSES}\n\n`;
    
    guesses.forEach(guess => {
      guess.split('').forEach((letter, i) => {
        const status = getLetterStatus(letter, i, guess, targetWord);
        if (status === 'correct') text += '🟩';
        else if (status === 'present') text += '🟨';
        else text += '⬜';
      });
      text += '\n';
    });
    text += '\n#OneSpace #CyberWordle';
    
    navigator.clipboard.writeText(text);
    alert(t('copiedToClipboard', 'Result copied to clipboard!'));
  };

  // --- Render Helpers ---
  const rows = useMemo(() => {
    const r = [...guesses];
    if (r.length < MAX_GUESSES && gameState === 'PLAYING') {
      r.push(currentGuess.padEnd(WORD_LENGTH, ' '));
    }
    while (r.length < MAX_GUESSES) {
      r.push(' '.repeat(WORD_LENGTH));
    }
    return r;
  }, [guesses, currentGuess, gameState]);

  const keys = [
    ['Q', 'W', 'E', 'R', 'T', 'Y', 'U', 'I', 'O', 'P'],
    ['A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L'],
    ['ENTER', 'Z', 'X', 'C', 'V', 'B', 'N', 'M', 'BACKSPACE']
  ];

  return (
    <div className="h-full flex flex-col bg-background select-none overflow-hidden relative">
      <header className="flex items-center justify-between p-4 border-b bg-background/80 backdrop-blur z-10">
        <div className="flex items-center gap-4">
          <button onClick={onBack} className="p-2 hover:bg-muted rounded-full transition-colors">
            <ArrowLeft className="w-5 h-5" />
          </button>
          <div>
            <h2 className="text-lg font-bold">{t('cyberWordle', 'Cyber Wordle')}</h2>
            <div className="flex items-center gap-3 text-xs text-muted-foreground">
              <button 
                onClick={() => initGame('DAILY')} 
                className={`uppercase font-bold transition-colors ${mode === 'DAILY' ? 'text-primary' : 'hover:text-foreground'}`}
              >
                {t('wordleDaily', 'Daily')}
              </button>
              <span className="opacity-20">|</span>
              <button 
                onClick={() => initGame('PRACTICE')} 
                className={`uppercase font-bold transition-colors ${mode === 'PRACTICE' ? 'text-primary' : 'hover:text-foreground'}`}
              >
                {t('wordlePractice', 'Practice')}
              </button>
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2">
            <button 
              onClick={() => setShowHelp(true)}
              className="p-2 hover:bg-muted rounded-full transition-colors text-muted-foreground hover:text-foreground"
              title={t('wordleHowToPlay', 'How to Play')}
            >
              <HelpCircle className="w-5 h-5" />
            </button>
            <button onClick={() => setIsMuted(!isMuted)} className="p-2 hover:bg-muted rounded-full transition-colors">
                {isMuted ? <VolumeX className="w-5 h-5" /> : <Volume2 className="w-5 h-5" />}
            </button>
        </div>
      </header>

      <main className="flex-1 flex flex-col items-center justify-center p-4 gap-8 overflow-y-auto">
        {/* Grid */}
        <div className="grid grid-rows-6 gap-2">
          {rows.map((guess, rowIndex) => (
            <div 
                key={rowIndex} 
                className={`flex gap-2 ${shakeRow === rowIndex ? 'animate-shake' : ''}`}
            >
              {guess.split('').map((letter, colIndex) => {
                const isRevealed = rowIndex < guesses.length;
                const status = isRevealed ? getLetterStatus(letter, colIndex, guess, targetWord) : 'unused';
                
                return (
                  <div 
                    key={colIndex}
                    className={`
                      w-12 h-12 md:w-14 md:h-14 flex items-center justify-center text-2xl font-black rounded-md border-2 transition-all duration-500
                      ${isRevealed ? 'rotate-x-180' : ''}
                      ${status === 'correct' ? 'bg-green-500 border-green-500 text-white' : 
                        status === 'present' ? 'bg-yellow-500 border-yellow-500 text-white' : 
                        status === 'absent' ? 'bg-muted-foreground/20 border-muted-foreground/20 text-muted-foreground' : 
                        letter !== ' ' ? 'border-foreground/40 scale-105' : 'border-muted/30'}
                    `}
                    style={{ transitionDelay: `${colIndex * 100}ms` }}
                  >
                    {letter.trim()}
                  </div>
                );
              })}
            </div>
          ))}
        </div>

        {/* Keyboard */}
        <div className="w-full max-w-lg flex flex-col gap-2">
          {keys.map((row, i) => (
            <div key={i} className="flex justify-center gap-1.5">
              {row.map(key => {
                const status = getKeyboardStatus[key] || 'unused';
                const isSpecial = key === 'ENTER' || key === 'BACKSPACE';
                
                return (
                  <button
                    key={key}
                    onClick={() => onKeyPress(key)}
                    className={`
                      h-14 flex items-center justify-center rounded font-bold text-sm transition-all
                      ${isSpecial ? 'px-4 text-[10px]' : 'w-10'}
                      ${status === 'correct' ? 'bg-green-500 text-white' :
                        status === 'present' ? 'bg-yellow-500 text-white' :
                        status === 'absent' ? 'bg-muted-foreground/20 text-muted-foreground opacity-50' :
                        'bg-muted hover:bg-muted-foreground/20'}
                    `}
                  >
                    {key === 'BACKSPACE' ? <Delete className="w-4 h-4" /> : 
                     key === 'ENTER' ? <CornerDownLeft className="w-4 h-4" /> : key}
                  </button>
                );
              })}
            </div>
          ))}
        </div>
      </main>

      {/* Result Overlay */}
      {gameState !== 'PLAYING' && (
        <div className="absolute inset-0 bg-background/90 backdrop-blur-md z-50 flex flex-col items-center justify-center p-8 text-center animate-in fade-in duration-500">
          <div className={`w-20 h-20 rounded-full flex items-center justify-center mb-6 ${gameState === 'WON' ? 'bg-primary/10 text-primary' : 'bg-destructive/10 text-destructive'}`}>
            {gameState === 'WON' ? <Trophy className="w-10 h-10" /> : <HelpCircle className="w-10 h-10" />}
          </div>
          <h1 className="text-5xl font-black tracking-tighter mb-2 italic">
            {gameState === 'WON' ? t('excellent', 'EXCELLENT!') : t('wordleReveal', 'GAME OVER')}
          </h1>
          <p className="text-muted-foreground mb-4 uppercase tracking-widest font-bold">
            {gameState === 'LOST' && t('wordleAnswerWas', 'The word was')}
          </p>
          {gameState === 'LOST' && (
              <div className="text-4xl font-black text-primary mb-8 tracking-widest">{targetWord}</div>
          )}
          
          <div className="grid grid-cols-3 gap-8 mb-12">
            <div className="text-center">
              <div className="text-[10px] text-muted-foreground uppercase mb-1">{t('wordlePlayed', 'Played')}</div>
              <div className="text-2xl font-mono font-bold">{stats.games_played}</div>
            </div>
            <div className="text-center border-x px-8">
              <div className="text-[10px] text-muted-foreground uppercase mb-1">{t('wordleWinRate', 'Win %')}</div>
              <div className="text-2xl font-mono font-bold">{stats.win_rate}</div>
            </div>
            <div className="text-center">
              <div className="text-[10px] text-muted-foreground uppercase mb-1">{t('wordleStreak', 'Streak')}</div>
              <div className="text-2xl font-mono font-bold">{stats.current_streak}</div>
            </div>
          </div>

          <div className="flex gap-4">
            <button onClick={shareResult} className="px-8 py-3 rounded-full border bg-card hover:bg-muted font-bold transition-all flex items-center gap-2">
              <Share2 className="w-4 h-4" /> {t('wordleShare', 'SHARE')}
            </button>
            <button onClick={() => initGame('PRACTICE')} className="px-10 py-3 rounded-full bg-primary text-primary-foreground font-bold hover:scale-105 transition-all shadow-xl shadow-primary/20 flex items-center gap-2">
              <RotateCcw className="w-4 h-4" /> {t('playAgain', 'PLAY AGAIN')}
            </button>
          </div>
        </div>
      )}

      {/* Help Overlay */}
      {showHelp && (
        <div className="absolute inset-0 bg-background/95 backdrop-blur-md z-[60] flex items-center justify-center p-6 animate-in fade-in zoom-in duration-200">
          <div className="max-w-sm w-full bg-card border rounded-2xl p-6 shadow-2xl relative">
            <button 
              onClick={() => setShowHelp(false)}
              className="absolute top-4 right-4 p-1 hover:bg-muted rounded-full transition-colors"
            >
              <RotateCcw className="w-5 h-5 rotate-45" /> {/* Close icon using rotated Lucide */}
            </button>
            
            <h2 className="text-xl font-black mb-4 uppercase tracking-tight">{t('wordleHowToPlay', 'How to Play')}</h2>
            
            <ul className="text-sm space-y-3 text-muted-foreground mb-6 text-left list-disc pl-4">
              <li>{t('wordleRule1', 'Guess the word in 6 tries.')}</li>
              <li>{t('wordleRule2', 'Each guess must be a valid 5-letter word.')}</li>
              <li>{t('wordleRule3', 'The color of the tiles will change to show how close your guess was to the word.')}</li>
            </ul>

            <div className="space-y-4 border-t pt-4">
              <div className="flex flex-col gap-2">
                <div className="flex gap-1.5">
                  <div className="w-8 h-8 flex items-center justify-center bg-green-500 text-white font-bold rounded">W</div>
                  <div className="w-8 h-8 flex items-center justify-center border-2 rounded">O</div>
                  <div className="w-8 h-8 flex items-center justify-center border-2 rounded">R</div>
                  <div className="w-8 h-8 flex items-center justify-center border-2 rounded">D</div>
                  <div className="w-8 h-8 flex items-center justify-center border-2 rounded">S</div>
                </div>
                <p className="text-xs text-muted-foreground">{t('wordleExampleCorrect', 'W is in the word and in the correct spot.')}</p>
              </div>

              <div className="flex flex-col gap-2">
                <div className="flex gap-1.5">
                  <div className="w-8 h-8 flex items-center justify-center border-2 rounded">L</div>
                  <div className="w-8 h-8 flex items-center justify-center bg-yellow-500 text-white font-bold rounded">I</div>
                  <div className="w-8 h-8 flex items-center justify-center border-2 rounded">G</div>
                  <div className="w-8 h-8 flex items-center justify-center border-2 rounded">H</div>
                  <div className="w-8 h-8 flex items-center justify-center border-2 rounded">T</div>
                </div>
                <p className="text-xs text-muted-foreground">{t('wordleExamplePresent', 'I is in the word but in the wrong spot.')}</p>
              </div>

              <div className="flex flex-col gap-2">
                <div className="flex gap-1.5">
                  <div className="w-8 h-8 flex items-center justify-center border-2 rounded">C</div>
                  <div className="w-8 h-8 flex items-center justify-center border-2 rounded">L</div>
                  <div className="w-8 h-8 flex items-center justify-center border-2 rounded">O</div>
                  <div className="w-8 h-8 flex items-center justify-center border-2 rounded">U</div>
                  <div className="w-8 h-8 flex items-center justify-center bg-muted-foreground/20 text-muted-foreground font-bold rounded">D</div>
                </div>
                <p className="text-xs text-muted-foreground">{t('wordleExampleAbsent', 'U is not in the word in any spot.')}</p>
              </div>
            </div>

            <button 
              onClick={() => setShowHelp(false)}
              className="mt-8 w-full py-3 bg-primary text-primary-foreground font-bold rounded-xl hover:opacity-90 transition-opacity"
            >
              OK
            </button>
          </div>
        </div>
      )}

      <style>{`
        @keyframes shake {
          0%, 100% { transform: translateX(0); }
          20%, 60% { transform: translateX(-5px); }
          40%, 80% { transform: translateX(5px); }
        }
        .animate-shake {
          animation: shake 0.5s ease-in-out;
        }
        .rotate-x-180 {
          transform: rotateX(180deg);
        }
      `}</style>
    </div>
  );
};

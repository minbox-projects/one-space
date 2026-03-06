import { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { ArrowLeft, Volume2, VolumeX, Play, RotateCcw, Pause } from 'lucide-react';
import { useTheme } from '../../ThemeProvider';
import { tetrisAudio } from './TetrisAudio';

// --- Types ---
type Point = { x: number; y: number };
type TetrominoType = 'I' | 'J' | 'L' | 'O' | 'S' | 'T' | 'Z';
type GameState = 'IDLE' | 'PLAYING' | 'PAUSED' | 'GAME_OVER';

interface Piece {
    type: TetrominoType;
    pos: Point;
    shape: number[][];
    color: string;
}

interface GameData {
    tetris_high_score?: number;
}

// --- Constants ---
const COLS = 20;
const ROWS = 22;
const BLOCK_SIZE = 25;
const CANVAS_WIDTH = COLS * BLOCK_SIZE;
const CANVAS_HEIGHT = ROWS * BLOCK_SIZE;

const TETROMINOS: Record<TetrominoType, { shape: number[][]; color: string }> = {
    I: { shape: [[0,0,0,0], [1,1,1,1], [0,0,0,0], [0,0,0,0]], color: '#22d3ee' }, // Cyan
    J: { shape: [[1,0,0], [1,1,1], [0,0,0]], color: '#3b82f6' }, // Blue
    L: { shape: [[0,0,1], [1,1,1], [0,0,0]], color: '#f97316' }, // Orange
    O: { shape: [[1,1], [1,1]], color: '#fbbf24' }, // Yellow
    S: { shape: [[0,1,1], [1,1,0], [0,0,0]], color: '#4ade80' }, // Green
    T: { shape: [[0,1,0], [1,1,1], [0,0,0]], color: '#a855f7' }, // Purple
    Z: { shape: [[1,1,0], [0,1,1], [0,0,0]], color: '#ef4444' }, // Red
};

export const TetrisGame = ({ onBack }: { onBack: () => void }) => {
    const { t } = useTranslation();
    const { theme } = useTheme();
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const nextCanvasRef = useRef<HTMLCanvasElement>(null);
    const holdCanvasRef = useRef<HTMLCanvasElement>(null);

    // --- State ---
    const [gameState, setGameState] = useState<GameState>('IDLE');
    const [score, setScore] = useState(0);
    const [highScore, setHighScore] = useState(0);
    const [level, setLevel] = useState(1);
    const [lines, setLines] = useState(0);
    const [isMuted, setIsMuted] = useState(false);

    // --- Refs for Game Logic ---
    const grid = useRef<string[][]>(Array.from({ length: ROWS }, () => Array(COLS).fill('')));
    const currentPiece = useRef<Piece | null>(null);
    const nextQueue = useRef<TetrominoType[]>([]);
    const holdPiece = useRef<TetrominoType | null>(null);
    const canHold = useRef(true);
    const lastDropTime = useRef(0);
    const dropInterval = useRef(1000);
    const animationFrameId = useRef<number | null>(null);
    const isTauri = '__TAURI_INTERNALS__' in window;

    // --- Helpers ---
    const createPiece = (type: TetrominoType): Piece => {
        const { shape, color } = TETROMINOS[type];
        return {
            type,
            pos: { x: Math.floor(COLS / 2) - Math.floor(shape[0].length / 2), y: type === 'I' ? -1 : 0 },
            shape: shape.map(row => [...row]),
            color
        };
    };

    const spawnPiece = useCallback(() => {
        if (nextQueue.current.length < 4) {
            const bag = (Object.keys(TETROMINOS) as TetrominoType[]).sort(() => Math.random() - 0.5);
            nextQueue.current.push(...bag);
        }
        const type = nextQueue.current.shift()!;
        const piece = createPiece(type);
        
        if (checkCollision(piece.pos, piece.shape)) {
            gameOver();
            return;
        }
        currentPiece.current = piece;
        canHold.current = true;
    }, []);

    const checkCollision = (pos: Point, shape: number[][]): boolean => {
        for (let y = 0; y < shape.length; y++) {
            for (let x = 0; x < shape[y].length; x++) {
                if (shape[y][x]) {
                    const newX = pos.x + x;
                    const newY = pos.y + y;
                    if (newX < 0 || newX >= COLS || newY >= ROWS) return true;
                    if (newY >= 0 && grid.current[newY][newX]) return true;
                }
            }
        }
        return false;
    };

    const rotatePiece = () => {
        if (!currentPiece.current) return;
        const p = currentPiece.current;
        const newShape = p.shape[0].map((_, i) => p.shape.map(row => row[i]).reverse());
        
        // Simple wall kick
        const offsets = [0, 1, -1, 2, -2];
        for (const offset of offsets) {
            if (!checkCollision({ x: p.pos.x + offset, y: p.pos.y }, newShape)) {
                p.pos.x += offset;
                p.shape = newShape;
                tetrisAudio.playRotate();
                return;
            }
        }
    };

    const lockPiece = () => {
        if (!currentPiece.current) return;
        const p = currentPiece.current;
        p.shape.forEach((row, y) => {
            row.forEach((val, x) => {
                if (val) {
                    const gridY = p.pos.y + y;
                    const gridX = p.pos.x + x;
                    if (gridY >= 0) {
                        grid.current[gridY][gridX] = p.color;
                    }
                }
            });
        });
        
        clearLines();
        spawnPiece();
        tetrisAudio.playHardDrop();
    };

    const clearLines = () => {
        let linesCleared = 0;
        for (let y = ROWS - 1; y >= 0; y--) {
            if (grid.current[y].every(cell => cell !== '')) {
                grid.current.splice(y, 1);
                grid.current.unshift(Array(COLS).fill(''));
                linesCleared++;
                y++; // Check same row again
            }
        }
        
        if (linesCleared > 0) {
            const linePoints = [0, 100, 300, 500, 800];
            const gainedPoints = linePoints[linesCleared] * level;
            setScore(prev => prev + gainedPoints);
            setLines(prev => {
                const newLines = prev + linesCleared;
                const newLevel = Math.floor(newLines / 10) + 1;
                if (newLevel > level) {
                    setLevel(newLevel);
                    dropInterval.current = Math.max(100, 1000 - (newLevel - 1) * 100);
                    tetrisAudio.playLevelUp();
                }
                return newLines;
            });
            tetrisAudio.playClear(linesCleared);
        }
    };

    const hardDrop = () => {
        if (!currentPiece.current) return;
        while (!checkCollision({ x: currentPiece.current.pos.x, y: currentPiece.current.pos.y + 1 }, currentPiece.current.shape)) {
            currentPiece.current.pos.y++;
        }
        lockPiece();
    };

    const hold = () => {
        if (!canHold.current || !currentPiece.current) return;
        const currentType = currentPiece.current.type;
        if (holdPiece.current) {
            const nextType = holdPiece.current;
            holdPiece.current = currentType;
            currentPiece.current = createPiece(nextType);
        } else {
            holdPiece.current = currentType;
            spawnPiece();
        }
        canHold.current = false;
        tetrisAudio.playHold();
    };

    const getGhostPos = (): Point => {
        if (!currentPiece.current) return { x: 0, y: 0 };
        const pos = { ...currentPiece.current.pos };
        while (!checkCollision({ x: pos.x, y: pos.y + 1 }, currentPiece.current.shape)) {
            pos.y++;
        }
        return pos;
    };

    const resetGame = useCallback(() => {
        grid.current = Array.from({ length: ROWS }, () => Array(COLS).fill(''));
        nextQueue.current = [];
        holdPiece.current = null;
        setScore(0);
        setLevel(1);
        setLines(0);
        dropInterval.current = 1000;
        spawnPiece();
        setGameState('PLAYING');
    }, [spawnPiece]);

    const gameOver = useCallback(async () => {
        setGameState('GAME_OVER');
        tetrisAudio.playGameOver();
        if (score > highScore) {
            setHighScore(score);
            if (isTauri) {
                try {
                    const dataStr = await invoke<string>('read_game_data');
                    let data: GameData = {};
                    try { data = JSON.parse(dataStr); } catch {}
                    if (Array.isArray(data)) data = {};
                    data.tetris_high_score = score;
                    await invoke('save_game_data', { dataJson: JSON.stringify(data) });
                } catch (e) { console.error(e); }
            } else {
                localStorage.setItem('onespace_tetris_highscore', score.toString());
            }
        }
    }, [score, highScore, isTauri]);

    const update = (timestamp: number) => {
        if (gameState !== 'PLAYING') return;

        if (timestamp - lastDropTime.current > dropInterval.current) {
            if (!currentPiece.current) {
                spawnPiece();
            } else if (!checkCollision({ x: currentPiece.current.pos.x, y: currentPiece.current.pos.y + 1 }, currentPiece.current.shape)) {
                currentPiece.current.pos.y++;
            } else {
                lockPiece();
            }
            lastDropTime.current = timestamp;
        }

        draw();
        animationFrameId.current = requestAnimationFrame(update);
    };

    // --- Rendering ---
    const draw = () => {
        const canvas = canvasRef.current;
        if (!canvas) return;
        const ctx = canvas.getContext('2d');
        if (!ctx) return;

        const isDark = theme === 'dark' || (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
        
        ctx.clearRect(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT);

        // Draw Grid
        ctx.strokeStyle = isDark ? 'rgba(255, 255, 255, 0.05)' : 'rgba(0, 0, 0, 0.05)';
        for (let x = 0; x <= CANVAS_WIDTH; x += BLOCK_SIZE) {
            ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, CANVAS_HEIGHT); ctx.stroke();
        }
        for (let y = 0; y <= CANVAS_HEIGHT; y += BLOCK_SIZE) {
            ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(CANVAS_WIDTH, y); ctx.stroke();
        }

        // Draw Locked Blocks
        grid.current.forEach((row, y) => {
            row.forEach((color, x) => {
                if (color) {
                    drawBlock(ctx, x, y, color);
                }
            });
        });

        // Draw Ghost
        if (currentPiece.current) {
            const ghostPos = getGhostPos();
            ctx.globalAlpha = 0.2;
            currentPiece.current.shape.forEach((row, y) => {
                row.forEach((val, x) => {
                    if (val) {
                        drawBlock(ctx, ghostPos.x + x, ghostPos.y + y, currentPiece.current!.color);
                    }
                });
            });
            ctx.globalAlpha = 1.0;

            // Draw Current Piece
            currentPiece.current.shape.forEach((row, y) => {
                row.forEach((val, x) => {
                    if (val) {
                        drawBlock(ctx, currentPiece.current!.pos.x + x, currentPiece.current!.pos.y + y, currentPiece.current!.color);
                    }
                });
            });
        }

        drawSideCanvas(nextCanvasRef, nextQueue.current[0]);
        drawSideCanvas(holdCanvasRef, holdPiece.current);
    };

    const drawBlock = (ctx: CanvasRenderingContext2D, x: number, y: number, color: string) => {
        if (y < 0) return;
        ctx.fillStyle = color;
        ctx.fillRect(x * BLOCK_SIZE + 1, y * BLOCK_SIZE + 1, BLOCK_SIZE - 2, BLOCK_SIZE - 2);
        
        // Highlight for 3D effect
        ctx.fillStyle = 'rgba(255, 255, 255, 0.3)';
        ctx.fillRect(x * BLOCK_SIZE + 1, y * BLOCK_SIZE + 1, BLOCK_SIZE - 2, 2);
        ctx.fillRect(x * BLOCK_SIZE + 1, y * BLOCK_SIZE + 1, 2, BLOCK_SIZE - 2);
    };

    const drawSideCanvas = (ref: React.RefObject<HTMLCanvasElement | null>, type: TetrominoType | null) => {
        const canvas = ref.current;
        if (!canvas) return;
        const ctx = canvas.getContext('2d');
        if (!ctx) return;
        ctx.clearRect(0, 0, canvas.width, canvas.height);
        if (!type) return;

        const { shape, color } = TETROMINOS[type];
        const size = 20;
        const offsetX = (canvas.width - shape[0].length * size) / 2;
        const offsetY = (canvas.height - shape.length * size) / 2;

        ctx.fillStyle = color;
        shape.forEach((row, y) => {
            row.forEach((val, x) => {
                if (val) {
                    ctx.fillRect(offsetX + x * size + 1, offsetY + y * size + 1, size - 2, size - 2);
                }
            });
        });
    };

    // --- Effects ---
    useEffect(() => {
        animationFrameId.current = requestAnimationFrame(update);
        return () => { if (animationFrameId.current !== null) cancelAnimationFrame(animationFrameId.current); };
    }, [gameState]);

    useEffect(() => {
        const handleKey = (e: KeyboardEvent) => {
            if (gameState === 'IDLE' || gameState === 'GAME_OVER') {
                if (e.code === 'Space') resetGame();
                return;
            }
            if (e.code === 'KeyP') {
                setGameState(prev => prev === 'PLAYING' ? 'PAUSED' : 'PLAYING');
                return;
            }
            if (gameState !== 'PLAYING' || !currentPiece.current) return;

            switch(e.code) {
                case 'ArrowLeft':
                case 'KeyA':
                    if (!checkCollision({ x: currentPiece.current.pos.x - 1, y: currentPiece.current.pos.y }, currentPiece.current.shape)) {
                        currentPiece.current.pos.x--;
                        tetrisAudio.playMove();
                    }
                    break;
                case 'ArrowRight':
                case 'KeyD':
                    if (!checkCollision({ x: currentPiece.current.pos.x + 1, y: currentPiece.current.pos.y }, currentPiece.current.shape)) {
                        currentPiece.current.pos.x++;
                        tetrisAudio.playMove();
                    }
                    break;
                case 'ArrowDown':
                case 'KeyS':
                    if (!checkCollision({ x: currentPiece.current.pos.x, y: currentPiece.current.pos.y + 1 }, currentPiece.current.shape)) {
                        currentPiece.current.pos.y++;
                        setScore(s => s + 1);
                        tetrisAudio.playMove();
                    }
                    break;
                case 'ArrowUp':
                case 'KeyW':
                case 'KeyX':
                    rotatePiece();
                    break;
                case 'Space':
                    hardDrop();
                    break;
                case 'KeyC':
                case 'ShiftLeft':
                case 'ShiftRight':
                    hold();
                    break;
            }
        };
        window.addEventListener('keydown', handleKey);
        return () => window.removeEventListener('keydown', handleKey);
    }, [gameState, resetGame]);

    useEffect(() => {
        const load = async () => {
            if (isTauri) {
                try {
                    const dataStr = await invoke<string>('read_game_data');
                    const data = JSON.parse(dataStr) as GameData;
                    if (data && data.tetris_high_score) setHighScore(data.tetris_high_score);
                } catch {}
            } else {
                const s = localStorage.getItem('onespace_tetris_highscore');
                if (s) setHighScore(parseInt(s));
            }
        };
        load();
    }, [isTauri]);

    useEffect(() => { tetrisAudio.setMuted(isMuted); }, [isMuted]);

    return (
        <div className="h-full flex flex-col bg-background select-none overflow-hidden relative">
            <header className="flex items-center justify-between p-4 border-b bg-background/80 backdrop-blur z-10">
                <div className="flex items-center gap-4">
                    <button onClick={onBack} className="p-2 hover:bg-muted rounded-full transition-colors">
                        <ArrowLeft className="w-5 h-5" />
                    </button>
                    <div>
                        <h2 className="text-lg font-bold">{t('cyberTetris', 'Cyber Tetris')}</h2>
                        <p className="text-xs text-muted-foreground">{t('tetrisDesc', 'Stack blocks, clear lines')}</p>
                    </div>
                </div>
                <div className="flex items-center gap-6">
                    <div className="text-right">
                        <div className="text-[10px] text-muted-foreground uppercase tracking-widest">{t('snakeScore', 'SCORE')}</div>
                        <div className="font-mono font-bold text-xl leading-none">{score}</div>
                    </div>
                    <div className="text-right border-l pl-4">
                        <div className="text-[10px] text-primary uppercase tracking-widest">{t('snakeHighScore', 'HI-SCORE')}</div>
                        <div className="font-mono font-bold text-xl leading-none text-primary">{highScore}</div>
                    </div>
                    <button onClick={() => setIsMuted(!isMuted)} className="p-2 hover:bg-muted rounded-full transition-colors">
                        {isMuted ? <VolumeX className="w-5 h-5" /> : <Volume2 className="w-5 h-5" />}
                    </button>
                </div>
            </header>

            <main className="flex-1 flex items-center justify-center p-4 bg-muted/5 gap-8 overflow-hidden">
                {/* Hold Area */}
                <div className="flex flex-col gap-4 items-center">
                    <div className="bg-card border rounded-xl p-3 w-24 text-center">
                        <div className="text-[10px] font-bold text-muted-foreground uppercase mb-2">{t('tetrisHold', 'HOLD')}</div>
                        <canvas ref={holdCanvasRef} width={80} height={80} className="bg-muted/20 rounded-lg" />
                    </div>
                    <div className="bg-card border rounded-xl p-3 w-24 text-center">
                        <div className="text-[10px] font-bold text-muted-foreground uppercase mb-2">{t('tetrisLevel', 'LEVEL')}</div>
                        <div className="text-2xl font-black font-mono">{level}</div>
                    </div>
                </div>

                {/* Main Game Canvas */}
                <div className="relative shadow-2xl rounded-lg overflow-hidden border-4 border-muted-foreground/10 bg-black/40">
                    <canvas ref={canvasRef} width={CANVAS_WIDTH} height={CANVAS_HEIGHT} className="block" />
                    {(gameState === 'IDLE' || gameState === 'GAME_OVER') && (
                        <div className="absolute inset-0 bg-black/80 backdrop-blur-sm flex flex-col items-center justify-center text-white p-6 text-center">
                            <h1 className="text-4xl font-black tracking-tighter mb-4 italic">
                                {gameState === 'GAME_OVER' ? t('snakeGameOver', 'GAME OVER') : t('cyberTetris', 'CYBER TETRIS')}
                            </h1>
                            {gameState === 'GAME_OVER' && (
                                <div className="mb-6 space-y-1">
                                    <p className="text-sm text-white/60">{t('snakeFinalScore', 'Final Score')}</p>
                                    <p className="text-3xl font-mono font-bold text-primary">{score}</p>
                                </div>
                            )}
                            <button onClick={resetGame} className="flex items-center gap-2 px-8 py-3 bg-primary text-primary-foreground font-bold rounded-full hover:scale-105 transition-transform shadow-lg shadow-primary/20">
                                {gameState === 'IDLE' ? <Play className="w-5 h-5" /> : <RotateCcw className="w-5 h-5" />}
                                {gameState === 'IDLE' ? t('snakeStart', 'START GAME') : t('snakeTryAgain', 'TRY AGAIN')}
                            </button>
                            <div className="mt-8 grid grid-cols-2 gap-x-8 gap-y-2 text-[10px] text-white/40 uppercase font-mono border-t border-white/10 pt-6">
                                <div>{t('tetrisControlsMove', 'Arrows: Move')}</div>
                                <div>{t('tetrisControlsRotate', 'Up/X: Rotate')}</div>
                                <div>{t('tetrisControlsDrop', 'Space: Drop')}</div>
                                <div>{t('tetrisControlsHold', 'C/Shift: Hold')}</div>
                            </div>
                        </div>
                    )}
                    {gameState === 'PAUSED' && (
                        <div className="absolute inset-0 bg-black/40 backdrop-blur-[2px] flex items-center justify-center">
                            <div className="flex flex-col items-center text-white">
                                <Pause className="w-12 h-12 mb-2 animate-pulse" />
                                <div className="font-bold tracking-widest">{t('snakePaused', 'PAUSED')}</div>
                            </div>
                        </div>
                    )}
                </div>

                {/* Next & Stats Area */}
                <div className="flex flex-col gap-4 items-center">
                    <div className="bg-card border rounded-xl p-3 w-24 text-center">
                        <div className="text-[10px] font-bold text-muted-foreground uppercase mb-2">{t('tetrisNext', 'NEXT')}</div>
                        <canvas ref={nextCanvasRef} width={80} height={80} className="bg-muted/20 rounded-lg" />
                    </div>
                    <div className="bg-card border rounded-xl p-3 w-24 text-center">
                        <div className="text-[10px] font-bold text-muted-foreground uppercase mb-2">{t('tetrisLines', 'LINES')}</div>
                        <div className="text-2xl font-black font-mono">{lines}</div>
                    </div>
                </div>
            </main>
        </div>
    );
};

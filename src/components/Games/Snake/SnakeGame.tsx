import { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { ArrowLeft, Volume2, VolumeX, Play, RotateCcw, Pause } from 'lucide-react';
import { useTheme } from '../../ThemeProvider';
import { snakeAudio } from './SynthAudio';

// --- Types ---
type Point = { x: number; y: number };
type Direction = 'UP' | 'DOWN' | 'LEFT' | 'RIGHT';
type GameState = 'IDLE' | 'PLAYING' | 'PAUSED' | 'GAME_OVER';
type PowerUpType = 'SPEED' | 'SHIELD' | 'BOMB' | 'MAGNET';

interface PowerUp {
  id: number;
  type: PowerUpType;
  pos: Point;
  expiresAt: number; // Game tick timestamp
}

interface Particle {
  id: number;
  x: number;
  y: number;
  vx: number;
  vy: number;
  life: number;
  color: string;
}

interface GameData {
  snake_high_score?: number;
}

// --- Constants ---
const GRID_SIZE = 20;
const INITIAL_SPEED = 150; // ms per tick
const MIN_SPEED = 60;
const CANVAS_WIDTH = 600;
const CANVAS_HEIGHT = 400;
const COLS = CANVAS_WIDTH / GRID_SIZE;
const ROWS = CANVAS_HEIGHT / GRID_SIZE;

export const SnakeGame = ({ onBack }: { onBack: () => void }) => {
  const { t } = useTranslation();
  const { theme } = useTheme();
  const canvasRef = useRef<HTMLCanvasElement>(null);

  // --- State ---
  const [gameState, setGameState] = useState<GameState>('IDLE');
  const [score, setScore] = useState(0);
  const [highScore, setHighScore] = useState(0);
  const [isMuted, setIsMuted] = useState(false);
  
  // Power-up UI State
  const [activePowerUps, setActivePowerUps] = useState<{type: PowerUpType, timeLeft: number}[]>([]);

  // --- Refs for Game Logic (avoid re-renders) ---
  const snake = useRef<Point[]>([{ x: 10, y: 10 }]);
  const direction = useRef<Direction>('RIGHT');
  const nextDirection = useRef<Direction>('RIGHT');
  const food = useRef<Point>({ x: 15, y: 10 });
  const obstacles = useRef<Point[]>([]);
  const powerUps = useRef<PowerUp[]>([]);
  const particles = useRef<Particle[]>([]);
  
  const lastTick = useRef(0);
  const speed = useRef(INITIAL_SPEED);
  const speedMultiplier = useRef(1); // From powerups
  const shieldActive = useRef(false);
  const magnetActive = useRef(false);
  const animationFrameId = useRef<number | null>(null);
  const isTauri = '__TAURI_INTERNALS__' in window;

  // --- Helpers ---
  const getRandomPos = useCallback((): Point => {
    return {
      x: Math.floor(Math.random() * COLS),
      y: Math.floor(Math.random() * ROWS)
    };
  }, []);

  const isCollision = (p1: Point, p2: Point) => p1.x === p2.x && p1.y === p2.y;

  const spawnFood = useCallback(() => {
    let newPos: Point;
    do {
      newPos = getRandomPos();
    } while (
      snake.current.some(s => isCollision(s, newPos)) ||
      obstacles.current.some(o => isCollision(o, newPos))
    );
    food.current = newPos;
  }, [getRandomPos]);

  const spawnObstacles = useCallback((count: number) => {
    for (let i = 0; i < count; i++) {
        let pos: Point;
        do {
            pos = getRandomPos();
        } while (
            snake.current.some(s => isCollision(s, pos)) ||
            isCollision(food.current, pos) ||
            // Don't spawn too close to head
            (Math.abs(pos.x - snake.current[0].x) < 3 && Math.abs(pos.y - snake.current[0].y) < 3)
        );
        obstacles.current.push(pos);
    }
  }, [getRandomPos]);

  const spawnPowerUp = useCallback(() => {
    if (Math.random() > 0.1) return; // 10% chance per tick check (controlled by caller)

    const types: PowerUpType[] = ['SPEED', 'SHIELD', 'BOMB', 'MAGNET'];
    const type = types[Math.floor(Math.random() * types.length)];
    let pos: Point;
    do {
      pos = getRandomPos();
    } while (
      snake.current.some(s => isCollision(s, pos)) ||
      obstacles.current.some(o => isCollision(o, pos)) ||
      isCollision(food.current, pos)
    );

    powerUps.current.push({
      id: Date.now(),
      type,
      pos,
      expiresAt: Date.now() + 10000 // Despawn after 10s
    });
  }, [getRandomPos]);

  const createParticles = (x: number, y: number, color: string, count = 5) => {
    for (let i = 0; i < count; i++) {
      particles.current.push({
        id: Math.random(),
        x: x * GRID_SIZE + GRID_SIZE / 2,
        y: y * GRID_SIZE + GRID_SIZE / 2,
        vx: (Math.random() - 0.5) * 4,
        vy: (Math.random() - 0.5) * 4,
        life: 1.0,
        color
      });
    }
  };

  // --- Game Loop Logic ---
  const resetGame = useCallback(() => {
    snake.current = [{ x: 10, y: 10 }, { x: 9, y: 10 }, { x: 8, y: 10 }];
    direction.current = 'RIGHT';
    nextDirection.current = 'RIGHT';
    score === 0 ? setScore(0) : setScore(0); // Force re-render if needed
    speed.current = INITIAL_SPEED;
    obstacles.current = [];
    powerUps.current = [];
    particles.current = [];
    shieldActive.current = false;
    magnetActive.current = false;
    speedMultiplier.current = 1;
    setActivePowerUps([]);
    
    spawnFood();
    spawnObstacles(3); // Initial obstacles
    setGameState('PLAYING');
  }, [spawnFood, spawnObstacles, score]);

  const gameOver = useCallback(async () => {
    setGameState('GAME_OVER');
    snakeAudio.playCrash();

    // Check High Score
    if (score > highScore) {
      setHighScore(score);
      // Save
      if (isTauri) {
        try {
            const dataStr = await invoke<string>('read_game_data');
            let data: GameData = {};
            try { data = JSON.parse(dataStr); } catch {}
            if (Array.isArray(data)) data = {}; // Handle legacy empty array
            
            data.snake_high_score = score;
            await invoke('save_game_data', { dataJson: JSON.stringify(data) });
        } catch (e) {
            console.error(e);
        }
      } else {
          localStorage.setItem('onespace_snake_highscore', score.toString());
      }
    }
  }, [score, highScore, isTauri]);

  const update = useCallback((timestamp: number) => {
    if (gameState !== 'PLAYING') return;

    // Throttle loop speed
    const currentSpeed = speed.current / speedMultiplier.current;
    if (timestamp - lastTick.current < currentSpeed) {
        // Still update particles for smoothness
        updateParticles();
        draw();
        animationFrameId.current = requestAnimationFrame(update);
        return;
    }
    lastTick.current = timestamp;

    // 1. Move Snake
    direction.current = nextDirection.current;
    const head = { ...snake.current[0] };

    switch (direction.current) {
      case 'UP': head.y -= 1; break;
      case 'DOWN': head.y += 1; break;
      case 'LEFT': head.x -= 1; break;
      case 'RIGHT': head.x += 1; break;
    }

    // 2. Collision Check (Walls)
    if (head.x < 0 || head.x >= COLS || head.y < 0 || head.y >= ROWS) {
        if (shieldActive.current) {
            shieldActive.current = false;
            snakeAudio.playShieldBreak();
            // Bounce back? Or just wrap? Let's stop at edge for now to simulate "save"
            // Simple logic: Reverse direction or just ignore move
            // Better: Teleport to opposite side as a "save" mechanic?
            // Or just consume shield and don't die, clamping position.
            head.x = Math.max(0, Math.min(head.x, COLS - 1));
            head.y = Math.max(0, Math.min(head.y, ROWS - 1));
             // Don't move this tick
        } else {
            gameOver();
            return;
        }
    }

    // 3. Collision Check (Self & Obstacles)
    if (
        snake.current.some(s => isCollision(s, head)) ||
        obstacles.current.some(o => isCollision(o, head))
    ) {
        if (shieldActive.current) {
             shieldActive.current = false;
             snakeAudio.playShieldBreak();
             // Consume shield, destroy obstacle if it was an obstacle
             const obsIndex = obstacles.current.findIndex(o => isCollision(o, head));
             if (obsIndex !== -1) {
                 obstacles.current.splice(obsIndex, 1);
                 createParticles(head.x, head.y, '#FF0000', 10);
             }
        } else {
            gameOver();
            return;
        }
    }

    // 4. Check Food
    let eaten = false;
    // Magnet Logic
    if (magnetActive.current) {
        const dist = Math.abs(head.x - food.current.x) + Math.abs(head.y - food.current.y);
        if (dist <= 3) {
            // Pull food towards head visual or just auto-eat? 
            // Let's just snap eat for simplicity in logic
            // Or move food closer? Let's just extend eating range effectively.
            if (dist > 0) {
                 // Move food 1 step closer
                 if (food.current.x < head.x) food.current.x++;
                 else if (food.current.x > head.x) food.current.x--;
                 
                 if (food.current.y < head.y) food.current.y++;
                 else if (food.current.y > head.y) food.current.y--;
            }
        }
    }

    if (isCollision(head, food.current)) {
      eaten = true;
      setScore(s => s + 10 * speedMultiplier.current);
      snakeAudio.playEat();
      createParticles(head.x, head.y, '#4ade80'); // Green
      spawnFood();
      
      // Speed up
      if (speed.current > MIN_SPEED) speed.current -= 2;
      
      // Maybe spawn obstacle
      if (Math.random() > 0.7) spawnObstacles(1);
      
      // Maybe spawn powerup
      spawnPowerUp();
    }

    // 5. Check PowerUps
    const powerUpIndex = powerUps.current.findIndex(p => isCollision(p.pos, head));
    if (powerUpIndex !== -1) {
        const p = powerUps.current[powerUpIndex];
        activatePowerUp(p.type);
        powerUps.current.splice(powerUpIndex, 1);
        snakeAudio.playPowerUp();
        createParticles(head.x, head.y, '#fbbf24', 8); // Gold
    }

    // Update Snake Body
    const newSnake = [head, ...snake.current];
    if (!eaten) newSnake.pop();
    snake.current = newSnake;

    // Clean up expired items
    const now = Date.now();
    powerUps.current = powerUps.current.filter(p => p.expiresAt > now);

    updateParticles();
    draw();
    animationFrameId.current = requestAnimationFrame(update);
  }, [gameState, gameOver, spawnFood, spawnObstacles, spawnPowerUp]);

  const activatePowerUp = (type: PowerUpType) => {
      const duration = type === 'MAGNET' ? 8000 : 5000;
      
      // Add to active list for UI
      setActivePowerUps(prev => {
          const filtered = prev.filter(p => p.type !== type);
          return [...filtered, { type, timeLeft: duration }];
      });

      // Apply Effect
      if (type === 'SPEED') speedMultiplier.current = 2;
      if (type === 'SHIELD') shieldActive.current = true;
      if (type === 'MAGNET') magnetActive.current = true;
      if (type === 'BOMB') {
          snakeAudio.playExplosion();
          createParticles(15, 10, '#ef4444', 50); // Big boom
          obstacles.current = []; // Clear all
      }

      // Set timeout to clear effect (except shield which is one-time use)
      if (type !== 'SHIELD' && type !== 'BOMB') {
          setTimeout(() => {
              if (type === 'SPEED') speedMultiplier.current = 1;
              if (type === 'MAGNET') magnetActive.current = false;
          }, duration);
      }
  };

  const updateParticles = () => {
      particles.current.forEach(p => {
          p.x += p.vx;
          p.y += p.vy;
          p.life -= 0.05;
      });
      particles.current = particles.current.filter(p => p.life > 0);
  };

  // --- Rendering ---
  const draw = () => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // Clear
    ctx.clearRect(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT);

    // Draw Grid (Cyber style)
    const isDark = theme === 'dark' || (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
    ctx.strokeStyle = isDark ? 'rgba(0, 255, 0, 0.05)' : 'rgba(0, 0, 0, 0.05)';
    ctx.lineWidth = 1;
    for (let x = 0; x <= CANVAS_WIDTH; x += GRID_SIZE) {
        ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, CANVAS_HEIGHT); ctx.stroke();
    }
    for (let y = 0; y <= CANVAS_HEIGHT; y += GRID_SIZE) {
        ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(CANVAS_WIDTH, y); ctx.stroke();
    }

    // Draw Obstacles
    ctx.fillStyle = isDark ? '#ef4444' : '#dc2626';
    obstacles.current.forEach(o => {
        ctx.fillRect(o.x * GRID_SIZE, o.y * GRID_SIZE, GRID_SIZE - 2, GRID_SIZE - 2);
    });

    // Draw Food
    ctx.fillStyle = isDark ? '#4ade80' : '#16a34a';
    ctx.shadowBlur = 10;
    ctx.shadowColor = isDark ? '#4ade80' : 'transparent';
    ctx.beginPath();
    ctx.arc(
        food.current.x * GRID_SIZE + GRID_SIZE/2, 
        food.current.y * GRID_SIZE + GRID_SIZE/2, 
        GRID_SIZE/2 - 2, 0, Math.PI * 2
    );
    ctx.fill();
    ctx.shadowBlur = 0;

    // Draw PowerUps
    powerUps.current.forEach(p => {
        ctx.fillStyle = '#fbbf24'; // Amber
        if (p.type === 'SHIELD') ctx.fillStyle = '#3b82f6'; // Blue
        if (p.type === 'BOMB') ctx.fillStyle = '#ef4444'; // Red
        
        ctx.beginPath();
        // Draw distinct shapes? Simple circle for now with letter
        ctx.arc(
            p.pos.x * GRID_SIZE + GRID_SIZE/2,
            p.pos.y * GRID_SIZE + GRID_SIZE/2,
            GRID_SIZE/2 - 2, 0, Math.PI * 2
        );
        ctx.fill();
        
        ctx.fillStyle = '#000';
        ctx.font = '10px monospace';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        let symbol = '?';
        if (p.type === 'SPEED') symbol = 'S';
        if (p.type === 'SHIELD') symbol = 'H';
        if (p.type === 'BOMB') symbol = 'B';
        if (p.type === 'MAGNET') symbol = 'M';
        ctx.fillText(symbol, p.pos.x * GRID_SIZE + GRID_SIZE/2, p.pos.y * GRID_SIZE + GRID_SIZE/2);
    });

    // Draw Snake
    snake.current.forEach((s, i) => {
        // Head
        if (i === 0) {
            ctx.fillStyle = isDark ? '#fff' : '#000';
            if (shieldActive.current) ctx.fillStyle = '#3b82f6'; // Blue head if shielded
        } else {
            ctx.fillStyle = isDark ? 'rgba(255, 255, 255, 0.5)' : 'rgba(0, 0, 0, 0.5)';
        }
        ctx.fillRect(s.x * GRID_SIZE, s.y * GRID_SIZE, GRID_SIZE - 2, GRID_SIZE - 2);
    });

    // Draw Particles
    particles.current.forEach(p => {
        ctx.globalAlpha = p.life;
        ctx.fillStyle = p.color;
        ctx.beginPath();
        ctx.arc(p.x, p.y, 2, 0, Math.PI * 2);
        ctx.fill();
        ctx.globalAlpha = 1.0;
    });
  };

  // --- Effects ---
  useEffect(() => {
    animationFrameId.current = requestAnimationFrame(update);
    return () => {
        if (animationFrameId.current !== null) cancelAnimationFrame(animationFrameId.current);
    };
  }, [update]);

  // Keyboard
  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (gameState === 'IDLE' || gameState === 'GAME_OVER') {
        if (e.code === 'Space') resetGame();
        return;
      }
      
      if (e.code === 'Space') {
          setGameState(prev => prev === 'PLAYING' ? 'PAUSED' : 'PLAYING');
          return;
      }

      if (gameState !== 'PLAYING') return;

      switch(e.code) {
        case 'ArrowUp': 
        case 'KeyW':
          if (direction.current !== 'DOWN') nextDirection.current = 'UP';
          break;
        case 'ArrowDown':
        case 'KeyS':
          if (direction.current !== 'UP') nextDirection.current = 'DOWN';
          break;
        case 'ArrowLeft':
        case 'KeyA':
          if (direction.current !== 'RIGHT') nextDirection.current = 'LEFT';
          break;
        case 'ArrowRight':
        case 'KeyD':
          if (direction.current !== 'LEFT') nextDirection.current = 'RIGHT';
          break;
      }
    };

    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [gameState, resetGame]);

  // Load High Score
  useEffect(() => {
      const load = async () => {
          if (isTauri) {
              try {
                  const dataStr = await invoke<string>('read_game_data');
                  const data = JSON.parse(dataStr) as GameData;
                  if (data && data.snake_high_score) setHighScore(data.snake_high_score);
              } catch {}
          } else {
              const s = localStorage.getItem('onespace_snake_highscore');
              if (s) setHighScore(parseInt(s));
          }
      };
      load();
  }, [isTauri]);

  // Audio mute sync
  useEffect(() => {
      snakeAudio.setMuted(isMuted);
  }, [isMuted]);

  return (
    <div className="h-full flex flex-col bg-background select-none overflow-hidden relative">
      {/* Header */}
      <header className="flex items-center justify-between p-4 border-b bg-background/80 backdrop-blur z-10">
        <div className="flex items-center gap-4">
          <button onClick={onBack} className="p-2 hover:bg-muted rounded-full transition-colors">
            <ArrowLeft className="w-5 h-5" />
          </button>
          <div>
            <h2 className="text-lg font-bold">{t('cyberSnake', 'Cyber Snake Pro')}</h2>
            <p className="text-xs text-muted-foreground">{t('snakeDesc', 'Eat, Grow, Survive')}</p>
          </div>
        </div>
        <div className="flex items-center gap-4">
            <div className="text-right">
                <div className="text-xs text-muted-foreground">{t('snakeScore', 'SCORE')}</div>
                <div className="font-mono font-bold text-xl leading-none">{score}</div>
            </div>
            <div className="text-right border-l pl-4">
                <div className="text-xs text-muted-foreground text-primary">{t('snakeHighScore', 'HI-SCORE')}</div>
                <div className="font-mono font-bold text-xl leading-none">{highScore}</div>
            </div>
            <button 
                onClick={() => setIsMuted(!isMuted)}
                className="p-2 hover:bg-muted rounded-full transition-colors ml-2"
            >
                {isMuted ? <VolumeX className="w-5 h-5" /> : <Volume2 className="w-5 h-5" />}
            </button>
        </div>
      </header>

      {/* Main Game Area */}
      <main className="flex-1 flex items-center justify-center p-4 bg-muted/10 relative">
        <div className="relative shadow-2xl rounded-lg overflow-hidden border border-border/50">
            <canvas 
                ref={canvasRef}
                width={CANVAS_WIDTH}
                height={CANVAS_HEIGHT}
                className="block bg-black/5 dark:bg-black/40"
            />
            
            {/* Overlay UI: Game Over / Menu */}
            {(gameState === 'IDLE' || gameState === 'GAME_OVER') && (
                <div className="absolute inset-0 bg-black/60 backdrop-blur-sm flex flex-col items-center justify-center text-white">
                    <h1 className="text-4xl font-black tracking-tighter mb-2">
                        {gameState === 'GAME_OVER' ? t('snakeGameOver', 'GAME OVER') : t('cyberSnake', 'CYBER SNAKE')}
                    </h1>
                    {gameState === 'GAME_OVER' && (
                        <p className="mb-6 text-xl">{t('snakeFinalScore', 'Final Score')}: <span className="text-primary font-mono">{score}</span></p>
                    )}
                    <button 
                        onClick={resetGame}
                        className="flex items-center gap-2 px-8 py-3 bg-primary text-primary-foreground font-bold rounded-full hover:scale-105 transition-transform"
                    >
                        {gameState === 'IDLE' ? <Play className="w-5 h-5" /> : <RotateCcw className="w-5 h-5" />}
                        {gameState === 'IDLE' ? t('snakeStart', 'START GAME') : t('snakeTryAgain', 'TRY AGAIN')}
                    </button>
                    <p className="mt-4 text-xs text-white/50 font-mono">{t('snakePressSpace', 'PRESS SPACE TO START')}</p>
                </div>
            )}

            {/* Overlay UI: Paused */}
            {gameState === 'PAUSED' && (
                <div className="absolute inset-0 bg-black/40 backdrop-blur-[2px] flex items-center justify-center">
                    <div className="flex flex-col items-center text-white">
                        <Pause className="w-12 h-12 mb-2 opacity-80" />
                        <div className="font-bold tracking-widest">{t('snakePaused', 'PAUSED')}</div>
                    </div>
                </div>
            )}
            
            {/* Active PowerUps HUD */}
            <div className="absolute top-2 left-2 flex gap-2">
                {activePowerUps.map((p, i) => (
                    <div key={i} className="flex items-center gap-1 bg-black/60 text-white text-xs px-2 py-1 rounded-full border border-white/10">
                        <span className={`w-2 h-2 rounded-full ${
                            p.type === 'SPEED' ? 'bg-yellow-400' : 
                            p.type === 'SHIELD' ? 'bg-blue-400' : 
                            p.type === 'MAGNET' ? 'bg-purple-400' : 'bg-red-400'
                        }`} />
                        <span>{p.type}</span>
                    </div>
                ))}
            </div>
        </div>
      </main>

      {/* Footer / Legend */}
      <footer className="p-4 border-t text-xs text-muted-foreground flex justify-center gap-8">
          <div className="flex items-center gap-1.5"><div className="w-3 h-3 bg-yellow-400 rounded-full opacity-80"></div> {t('snakeSpeed', 'Speed')}</div>
          <div className="flex items-center gap-1.5"><div className="w-3 h-3 bg-blue-400 rounded-full opacity-80"></div> {t('snakeShield', 'Shield')}</div>
          <div className="flex items-center gap-1.5"><div className="w-3 h-3 bg-red-400 rounded-full opacity-80"></div> {t('snakeBomb', 'Bomb')}</div>
          <div className="flex items-center gap-1.5"><div className="w-3 h-3 bg-purple-400 rounded-full opacity-80"></div> {t('snakeMagnet', 'Magnet')}</div>
      </footer>
    </div>
  );
};

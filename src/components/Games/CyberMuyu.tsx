import { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { ArrowLeft, Volume2, VolumeX } from 'lucide-react';
import { useTheme } from '../ThemeProvider';

interface FloatingText {
  id: number;
  x: number;
  y: number;
  text: string;
}

interface GameData {
  muyu_count?: number;
}

export const CyberMuyu = ({ onBack }: { onBack: () => void }) => {
  const { t } = useTranslation();
  const { theme } = useTheme();
  const [count, setCount] = useState<number>(0);
  const [floatingTexts, setFloatingTexts] = useState<FloatingText[]>([]);
  const [isPressed, setIsPressed] = useState(false);
  const [isMuted, setIsMuted] = useState(false);
  const isTauri = '__TAURI_INTERNALS__' in window;
  const initialized = useRef(false);
  const audioContextRef = useRef<AudioContext | null>(null);

  // Initialize Audio Context
  useEffect(() => {
    const AudioContext = window.AudioContext || (window as any).webkitAudioContext;
    if (AudioContext) {
      audioContextRef.current = new AudioContext();
    }
    return () => {
      if (audioContextRef.current) {
        audioContextRef.current.close();
      }
    };
  }, []);

  const playWoodfishSound = useCallback(() => {
    if (isMuted || !audioContextRef.current) return;
    
    const ctx = audioContextRef.current;
    if (ctx.state === 'suspended') {
      ctx.resume();
    }

    const t = ctx.currentTime;

    // Resonant Wood Body (Main "Tok")
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();

    // Use Triangle wave for softer, woodier tone than sine
    osc.type = 'triangle';
    
    // Pitch envelope: Start high, drop quickly (simulates initial strike resonance)
    osc.frequency.setValueAtTime(800, t);
    osc.frequency.exponentialRampToValueAtTime(300, t + 0.1);

    // Amplitude envelope: Sharp attack, fast exponential decay
    gain.gain.setValueAtTime(0, t);
    gain.gain.linearRampToValueAtTime(0.8, t + 0.01);
    gain.gain.exponentialRampToValueAtTime(0.01, t + 0.15);

    osc.connect(gain);
    gain.connect(ctx.destination);

    osc.start(t);
    osc.stop(t + 0.2);

    // Adding a second, higher partial for "brightness" (The "Click")
    const osc2 = ctx.createOscillator();
    const gain2 = ctx.createGain();

    osc2.type = 'sine';
    osc2.frequency.setValueAtTime(1200, t);
    
    gain2.gain.setValueAtTime(0, t);
    gain2.gain.linearRampToValueAtTime(0.3, t + 0.005);
    gain2.gain.exponentialRampToValueAtTime(0.001, t + 0.05); // Very short

    osc2.connect(gain2);
    gain2.connect(ctx.destination);

    osc2.start(t);
    osc2.stop(t + 0.1);

  }, [isMuted]);

  // Load data from Tauri storage
  useEffect(() => {
    const loadData = async () => {
      if (initialized.current) return;
      
      if (isTauri) {
        try {
          const dataStr = await invoke<string>('read_game_data');
          let data: GameData = {};
          try { 
              const parsed = JSON.parse(dataStr);
              if (!Array.isArray(parsed)) {
                  data = parsed as GameData;
              }
          } catch (e) { 
              console.error('JSON Parse error', e); 
          }

          if (data && typeof data.muyu_count === 'number') {
            setCount(data.muyu_count);
          }
          initialized.current = true;
        } catch (e) {
          console.error('Failed to load game data', e);
        }
      } else {
        const saved = localStorage.getItem('onespace_muyu_count');
        if (saved) setCount(parseInt(saved, 10));
        initialized.current = true;
      }
    };
    loadData();
  }, [isTauri]);

  // Save data helper - stable ref
  const saveToDisk = useCallback(async (newCount: number) => {
    if (isTauri) {
        try {
            let data: GameData = { muyu_count: newCount };
            try {
                const dataStr = await invoke<string>('read_game_data');
                const parsed = JSON.parse(dataStr);
                if (!Array.isArray(parsed)) {
                    data = { ...parsed, muyu_count: newCount };
                }
            } catch (e) { /* ignore read error, write fresh */ }

            await invoke('save_game_data', { dataJson: JSON.stringify(data) });
        } catch (err) {
            console.error('Failed to save game data', err);
        }
    } else {
      localStorage.setItem('onespace_muyu_count', newCount.toString());
    }
  }, [isTauri]);

  const handleStrike = useCallback((e?: React.MouseEvent | React.KeyboardEvent) => {
    if (e && 'button' in e && e.button !== 0) return;
    
    setCount(prev => {
        const next = prev + 1;
        saveToDisk(next); 
        return next;
    });
    
    setIsPressed(true);
    setTimeout(() => setIsPressed(false), 80);

    // Add floating text
    const id = Date.now();
    const newText: FloatingText = {
      id,
      x: Math.random() * 40 - 20, 
      y: 0,
      text: t('meritPlusOne', '功德 +1')
    };
    setFloatingTexts(prev => [...prev, newText]);
    setTimeout(() => {
      setFloatingTexts(prev => prev.filter(t => t.id !== id));
    }, 800);

    playWoodfishSound();
  }, [t, saveToDisk, playWoodfishSound]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.code === 'Space') {
        e.preventDefault();
        handleStrike();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleStrike]);

  // Determine theme-based colors
  const isDark = theme === 'dark' || (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
  
  // Theme colors for Woodfish
  const woodColor1 = isDark ? "#5d3c2e" : "#8B5A2B"; 
  const woodColor2 = isDark ? "#3d2217" : "#654321";
  const woodColor3 = isDark ? "#2a160f" : "#4A3018";
  const detailColor = isDark ? "#1a0f0a" : "#2F1B0C";

  return (
    <div className="h-full flex flex-col bg-background select-none overflow-hidden relative transition-colors duration-500">
      {/* Dynamic Background Overlay */}
      <div className={`absolute inset-0 pointer-events-none transition-opacity duration-500 ${isDark ? 'bg-gradient-to-b from-primary/5 via-transparent to-primary/5' : 'bg-gradient-to-b from-orange-50/50 via-white/20 to-orange-50/50'}`} />
      
      <header className="flex items-center justify-between p-4 border-b relative z-10 bg-background/80 backdrop-blur-sm transition-colors duration-300">
        <div className="flex items-center gap-4">
          <button 
            onClick={onBack}
            className="p-2 hover:bg-muted rounded-full transition-colors"
          >
            <ArrowLeft className="w-5 h-5" />
          </button>
          <div>
            <h2 className="text-lg font-bold">{t('cyberMuyu', '功德木鱼')}</h2>
            <p className="text-xs text-muted-foreground">{t('muyuDesc', '敲击木鱼，静心养性')}</p>
          </div>
        </div>
        <button 
          onClick={() => setIsMuted(!isMuted)}
          className="p-2 hover:bg-muted rounded-full transition-colors"
        >
          {isMuted ? <VolumeX className="w-5 h-5" /> : <Volume2 className="w-5 h-5" />}
        </button>
      </header>

      <main className="flex-1 flex flex-col items-center justify-center relative p-8 transition-colors duration-300">
        {/* Count Display */}
        <div className="absolute top-12 text-center z-10">
          <div className="text-sm text-muted-foreground/60 uppercase tracking-[0.2em] mb-2">{t('totalMerit', '当前功德')}</div>
          <div className={`text-6xl font-black font-mono tracking-tighter drop-shadow-sm transition-colors duration-300 ${isDark ? 'text-white' : 'text-foreground'}`}>{count}</div>
        </div>

        {/* Woodfish Container */}
        <div className="relative group cursor-pointer" onClick={(e) => handleStrike(e)}>
          {/* Floating Texts */}
          {floatingTexts.map(ft => (
            <div 
              key={ft.id}
              className="absolute left-1/2 -translate-x-1/2 pointer-events-none animate-float-up text-primary font-bold text-2xl whitespace-nowrap z-20"
              style={{ 
                transform: `translate(calc(-50% + ${ft.x}px), -120px)`
              }}
            >
              {ft.text}
            </div>
          ))}

          {/* Realistic Woodfish SVG */}
          <div className={`transition-all duration-75 ${isPressed ? 'scale-[0.92] brightness-90' : 'scale-100 active:scale-95'}`}>
            <svg width="280" height="220" viewBox="0 0 280 220" fill="none" xmlns="http://www.w3.org/2000/svg" className={`drop-shadow-xl filter transition-all duration-300 ${isDark ? 'drop-shadow-[0_25px_50px_rgba(0,0,0,0.5)]' : 'drop-shadow-[0_20px_40px_rgba(0,0,0,0.15)]'}`}>
              {/* Main Body */}
              <path d="M40 110C40 60 80 20 140 20C200 20 240 60 240 110C240 160 200 200 140 200C80 200 40 160 40 110Z" fill="url(#wood-grad)" />
              {/* Mouth Slit */}
              <path d="M80 140C100 130 180 130 200 140C190 155 110 155 80 140Z" fill={detailColor} />
              {/* Fish Eyes/Detail */}
              <circle cx="85" cy="80" r="8" fill={detailColor} opacity="0.4" />
              <circle cx="195" cy="80" r="8" fill={detailColor} opacity="0.4" />
              {/* Highlights */}
              <path d="M100 45C120 35 160 35 180 45" stroke="white" strokeWidth="2" strokeLinecap="round" opacity="0.1" />
              
              <defs>
                <linearGradient id="wood-grad" x1="0%" y1="0%" x2="100%" y2="100%">
                  <stop offset="0%" stopColor={woodColor1} />
                  <stop offset="50%" stopColor={woodColor2} />
                  <stop offset="100%" stopColor={woodColor3} />
                </linearGradient>
              </defs>
            </svg>
          </div>
          
          <div className="mt-16 text-center">
            <kbd className="px-4 py-1.5 bg-muted rounded border border-border text-xs font-mono text-muted-foreground/80 shadow-sm">
              SPACE
            </kbd>
            <p className="mt-4 text-sm text-muted-foreground/60 font-medium tracking-wide animate-pulse">{t('clickToStrike', '点击或按空格敲击')}</p>
          </div>
        </div>
      </main>

      <style>{`
        @keyframes float-up {
          0% { transform: translate(-50%, 0); opacity: 1; }
          100% { transform: translate(-50%, -180px); opacity: 0; }
        }
        .animate-float-up {
          animation: float-up 0.6s cubic-bezier(0.16, 1, 0.3, 1) forwards;
        }
      `}</style>
    </div>
  );
};

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Fish, Sparkles, Gamepad2, Play, LayoutGrid } from 'lucide-react';
import { CyberMuyu } from './Games/CyberMuyu';
import { SnakeGame } from './Games/Snake/SnakeGame';
import { TetrisGame } from './Games/Tetris/TetrisGame';

type GameId = 'muyu' | 'snake' | 'tetris' | 'none';

export const FishPond = () => {
  const { t } = useTranslation();
  const [activeGame, setActiveGame] = useState<GameId>('none');

  const games = [
    {
      id: 'muyu',
      name: t('cyberMuyu', 'Cyber Muyu'),
      desc: t('muyuDescShort', 'Electronic woodfish for meditation'),
      icon: Sparkles,
      color: 'bg-orange-500/10 text-orange-500',
      component: CyberMuyu
    },
    {
      id: 'snake',
      name: t('cyberSnake', 'Cyber Snake Pro'),
      desc: t('snakeDescShort', 'Classic snake with power-ups and obstacles'),
      icon: Gamepad2,
      color: 'bg-green-500/10 text-green-500',
      component: SnakeGame
    },
    {
      id: 'tetris',
      name: t('cyberTetris', 'Cyber Tetris'),
      desc: t('tetrisDescShort', 'Classic block stacking game'),
      icon: LayoutGrid,
      color: 'bg-purple-500/10 text-purple-500',
      component: TetrisGame
    },
    {
        id: 'minesweeper',
        name: t('minesweeper', 'Minesweeper'),
        desc: t('comingSoon', 'Coming Soon'),
        icon: Play,
        color: 'bg-blue-500/10 text-blue-500',
        disabled: true
    }
  ];

  if (activeGame === 'muyu') {
    return <CyberMuyu onBack={() => setActiveGame('none')} />;
  }

  if (activeGame === 'snake') {
    return <SnakeGame onBack={() => setActiveGame('none')} />;
  }

  if (activeGame === 'tetris') {
    return <TetrisGame onBack={() => setActiveGame('none')} />;
  }

  return (
    <div className="h-full flex flex-col bg-background p-8 overflow-y-auto">
      <header className="mb-10 text-center">
        <div className="inline-flex p-3 rounded-2xl bg-primary/10 text-primary mb-4">
          <Fish className="w-10 h-10" />
        </div>
        <h1 className="text-3xl font-bold tracking-tight mb-2">{t('fishPond', 'Fish Pond')}</h1>
        <p className="text-muted-foreground">{t('fishPondDesc', 'Take a break and relax here')}</p>
      </header>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6 max-w-5xl mx-auto w-full">
        {games.map((game) => (
          <button
            key={game.id}
            disabled={game.disabled}
            onClick={() => setActiveGame(game.id as GameId)}
            className={`flex flex-col items-start p-6 rounded-2xl border bg-card text-left transition-all duration-300 group ${
              game.disabled 
                ? 'opacity-60 cursor-not-allowed' 
                : 'hover:border-primary hover:shadow-xl hover:shadow-primary/5 hover:-translate-y-1'
            }`}
          >
            <div className={`p-3 rounded-xl mb-4 transition-transform group-hover:scale-110 ${game.color}`}>
              <game.icon className="w-6 h-6" />
            </div>
            <h3 className="text-lg font-bold mb-1 group-hover:text-primary transition-colors">{game.name}</h3>
            <p className="text-sm text-muted-foreground leading-relaxed">{game.desc}</p>
            
            {!game.disabled && (
              <div className="mt-6 flex items-center text-xs font-bold text-primary opacity-0 group-hover:opacity-100 transition-opacity">
                {t('playNow', 'Play Now')} →
              </div>
            )}
            {game.disabled && (
               <div className="mt-6 text-[10px] font-bold text-muted-foreground uppercase tracking-widest">
                {t('comingSoon', 'Coming Soon')}
               </div>
            )}
          </button>
        ))}
      </div>
      
      <footer className="mt-20 text-center text-xs text-muted-foreground border-t pt-8">
         <p>© {new Date().getFullYear()} OneSpace Fish Pond • {t('relaxAndWork', 'Relax for better work')}</p>
      </footer>
    </div>
  );
};

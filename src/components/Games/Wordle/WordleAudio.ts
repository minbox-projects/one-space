// src/components/Games/Wordle/WordleAudio.ts

class WordleAudio {
  private ctx: AudioContext | null = null;
  private isMuted: boolean = false;

  constructor() {
    this.init();
  }

  private init() {
    if (typeof window !== 'undefined') {
      const AudioContext = window.AudioContext || (window as any).webkitAudioContext;
      if (AudioContext) {
        this.ctx = new AudioContext();
      }
    }
  }

  public setMuted(muted: boolean) {
    this.isMuted = muted;
    if (this.ctx && this.ctx.state === 'suspended' && !muted) {
      this.ctx.resume();
    }
  }

  private playClick(freq: number, duration: number, gainValue: number = 0.1) {
    if (this.isMuted || !this.ctx) return;
    if (this.ctx.state === 'suspended') this.ctx.resume();

    const osc = this.ctx.createOscillator();
    const gainNode = this.ctx.createGain();
    const filter = this.ctx.createBiquadFilter();

    osc.type = 'square';
    osc.frequency.setValueAtTime(freq, this.ctx.currentTime);
    
    filter.type = 'lowpass';
    filter.frequency.setValueAtTime(freq * 2, this.ctx.currentTime);

    gainNode.gain.setValueAtTime(gainValue, this.ctx.currentTime);
    gainNode.gain.exponentialRampToValueAtTime(0.001, this.ctx.currentTime + duration);

    osc.connect(filter);
    filter.connect(gainNode);
    gainNode.connect(this.ctx.destination);

    osc.start();
    osc.stop(this.ctx.currentTime + duration);
  }

  public playKey() {
    // Mechanical key click (Blue switch style)
    this.playClick(1200 + Math.random() * 200, 0.05, 0.05);
  }

  public playEnter() {
    // Heavy enter key
    this.playClick(800, 0.1, 0.08);
  }

  public playReveal(index: number) {
    // Sequential reveal sound
    setTimeout(() => {
        this.playClick(1000 + index * 100, 0.08, 0.04);
    }, index * 100);
  }

  public playSuccess() {
    if (this.isMuted || !this.ctx) return;
    const now = this.ctx.currentTime;
    [523.25, 659.25, 783.99, 1046.50].forEach((f, i) => {
      const osc = this.ctx!.createOscillator();
      const g = this.ctx!.createGain();
      osc.frequency.setValueAtTime(f, now + i * 0.1);
      g.gain.setValueAtTime(0.05, now + i * 0.1);
      g.gain.exponentialRampToValueAtTime(0.001, now + i * 0.1 + 0.4);
      osc.connect(g);
      g.connect(this.ctx!.destination);
      osc.start(now + i * 0.1);
      osc.stop(now + i * 0.1 + 0.4);
    });
  }

  public playFail() {
    if (this.isMuted || !this.ctx) return;
    const now = this.ctx.currentTime;
    [200, 150, 100].forEach((f, i) => {
      const osc = this.ctx!.createOscillator();
      const g = this.ctx!.createGain();
      osc.type = 'sawtooth';
      osc.frequency.setValueAtTime(f, now + i * 0.1);
      g.gain.setValueAtTime(0.05, now + i * 0.1);
      g.gain.exponentialRampToValueAtTime(0.001, now + i * 0.1 + 0.3);
      osc.connect(g);
      g.connect(this.ctx!.destination);
      osc.start(now + i * 0.1);
      osc.stop(now + i * 0.1 + 0.3);
    });
  }

  public destroy() {
    if (this.ctx) {
      this.ctx.close();
      this.ctx = null;
    }
  }
}

export const wordleAudio = new WordleAudio();

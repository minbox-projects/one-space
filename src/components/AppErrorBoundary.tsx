import React from 'react';
import { AlertCircle, Check, Copy, RefreshCw } from 'lucide-react';

type AppErrorBoundaryProps = {
  label: string;
  resetKey?: string;
  children: React.ReactNode;
};

type AppErrorBoundaryState = {
  error: Error | null;
  copied: boolean;
};

export class AppErrorBoundary extends React.Component<
  AppErrorBoundaryProps,
  AppErrorBoundaryState
> {
  state: AppErrorBoundaryState = {
    error: null,
    copied: false,
  };

  static getDerivedStateFromError(error: Error): AppErrorBoundaryState {
    return { error, copied: false };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error(`[${this.props.label}] render crashed`, error, info);
  }

  componentDidUpdate(prevProps: AppErrorBoundaryProps) {
    if (prevProps.resetKey !== this.props.resetKey && this.state.error) {
      this.setState({ error: null, copied: false });
    }
  }

  handleReset = () => {
    this.setState({ error: null, copied: false });
  };

  handleCopy = async () => {
    const content = this.state.error?.stack || this.state.error?.message;
    if (!content) return;
    try {
      await navigator.clipboard.writeText(content);
      this.setState({ copied: true });
      window.setTimeout(() => {
        this.setState((current) => (current.error ? { ...current, copied: false } : current));
      }, 2000);
    } catch (error) {
      console.error('failed to copy error stack', error);
    }
  };

  render() {
    if (!this.state.error) {
      return this.props.children;
    }

    return (
      <div className="flex h-full items-center justify-center p-6">
        <div className="w-full max-w-2xl rounded-2xl border border-destructive/20 bg-card p-6 shadow-sm">
          <div className="flex items-start gap-3">
            <div className="rounded-full bg-destructive/10 p-2 text-destructive">
              <AlertCircle className="h-5 w-5" />
            </div>
            <div className="min-w-0 flex-1">
              <h2 className="text-lg font-semibold text-foreground">
                {this.props.label} 页面发生异常
              </h2>
              <p className="mt-2 text-sm text-muted-foreground">
                已拦截运行时错误，避免整页白屏。可以先重试；如果仍然失败，下面这段错误信息就是我们继续定位的依据。
              </p>
              <pre className="mt-4 overflow-auto rounded-xl bg-muted p-4 text-xs text-destructive whitespace-pre-wrap break-words select-text">
                {this.state.error.stack || this.state.error.message}
              </pre>
              <div className="mt-4 flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => {
                    void this.handleCopy();
                  }}
                  className="inline-flex items-center gap-2 rounded-md border px-4 py-2 text-sm font-medium hover:bg-muted"
                >
                  {this.state.copied ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
                  {this.state.copied ? '已复制' : '复制堆栈'}
                </button>
                <button
                  type="button"
                  onClick={this.handleReset}
                  className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
                >
                  <RefreshCw className="h-4 w-4" />
                  重试
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    );
  }
}

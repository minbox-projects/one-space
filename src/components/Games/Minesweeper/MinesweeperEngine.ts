// src/components/Games/Minesweeper/MinesweeperEngine.ts

export type CellValue = number | 'mine' | 'empty';

export interface Cell {
  value: CellValue;
  isRevealed: boolean;
  isFlagged: boolean;
  isExploded?: boolean;
}

export type MinesweeperBoard = Cell[][];

export class MinesweeperEngine {
  public static createBoard(rows: number, cols: number): MinesweeperBoard {
    return Array.from({ length: rows }, () =>
      Array.from({ length: cols }, () => ({
        value: 'empty',
        isRevealed: false,
        isFlagged: false,
      }))
    );
  }

  public static plantMines(
    board: MinesweeperBoard,
    minesCount: number,
    firstClickRow: number,
    firstClickCol: number
  ): MinesweeperBoard {
    const rows = board.length;
    const cols = board[0].length;
    let plantedMines = 0;

    const newBoard = board.map(row => row.map(cell => ({ ...cell })));

    while (plantedMines < minesCount) {
      const r = Math.floor(Math.random() * rows);
      const c = Math.floor(Math.random() * cols);

      // Don't plant mine on first click or its neighbors
      const isNeighbor = Math.abs(r - firstClickRow) <= 1 && Math.abs(c - firstClickCol) <= 1;

      if (newBoard[r][c].value !== 'mine' && !isNeighbor) {
        newBoard[r][c].value = 'mine';
        plantedMines++;
      }
    }

    // Calculate numbers
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        if (newBoard[r][c].value === 'mine') continue;

        let neighbors = 0;
        for (let dr = -1; r + dr >= 0 && r + dr < rows && dr <= 1; dr++) {
          for (let dc = -1; c + dc >= 0 && c + dc < cols && dc <= 1; dc++) {
            if (newBoard[r + dr][c + dc].value === 'mine') neighbors++;
          }
        }
        newBoard[r][c].value = neighbors === 0 ? 'empty' : neighbors;
      }
    }

    return newBoard;
  }

  public static revealCell(board: MinesweeperBoard, r: number, c: number): MinesweeperBoard {
    if (r < 0 || r >= board.length || c < 0 || c >= board[0].length) return board;
    if (board[r][c].isRevealed || board[r][c].isFlagged) return board;

    const newBoard = board.map(row => row.map(cell => ({ ...cell })));
    
    const reveal = (row: number, col: number) => {
      if (row < 0 || row >= newBoard.length || col < 0 || col >= newBoard[0].length) return;
      if (newBoard[row][col].isRevealed || newBoard[row][col].isFlagged) return;

      newBoard[row][col].isRevealed = true;

      if (newBoard[row][col].value === 'empty') {
        for (let dr = -1; dr <= 1; dr++) {
          for (let dc = -1; dc <= 1; dc++) {
            reveal(row + dr, col + dc);
          }
        }
      }
    };

    reveal(r, c);
    return newBoard;
  }

  public static toggleFlag(board: MinesweeperBoard, r: number, c: number): MinesweeperBoard {
    if (board[r][c].isRevealed) return board;
    const newBoard = board.map(row => row.map(cell => ({ ...cell })));
    newBoard[r][c].isFlagged = !newBoard[r][c].isFlagged;
    return newBoard;
  }

  public static checkWin(board: MinesweeperBoard, minesCount: number): boolean {
    let revealedCount = 0;
    const rows = board.length;
    const cols = board[0].length;

    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        if (board[r][c].isRevealed) revealedCount++;
      }
    }

    return revealedCount === rows * cols - minesCount;
  }
}

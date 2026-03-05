// src/components/Games/Sudoku/SudokuEngine.ts

export type SudokuBoard = (number | null)[][];

export class SudokuEngine {
  // Check if a number can be placed in a cell
  public static isValid(board: SudokuBoard, row: number, col: number, num: number): boolean {
    for (let x = 0; x < 9; x++) {
      if (board[row][x] === num) return false;
      if (board[x][col] === num) return false;
    }

    const startRow = row - (row % 3);
    const startCol = col - (col % 3);
    for (let i = 0; i < 3; i++) {
      for (let j = 0; j < 3; j++) {
        if (board[i + startRow][j + startCol] === num) return false;
      }
    }
    return true;
  }

  // Backtracking solver to fill the board
  public static solve(board: SudokuBoard): boolean {
    for (let row = 0; row < 9; row++) {
      for (let col = 0; col < 9; col++) {
        if (board[row][col] === null) {
          const nums = [1, 2, 3, 4, 5, 6, 7, 8, 9].sort(() => Math.random() - 0.5);
          for (const num of nums) {
            if (this.isValid(board, row, col, num)) {
              board[row][col] = num;
              if (this.solve(board)) return true;
              board[row][col] = null;
            }
          }
          return false;
        }
      }
    }
    return true;
  }

  // Generate a complete solved board
  public static generateSolvedBoard(): number[][] {
    const board: SudokuBoard = Array.from({ length: 9 }, () => Array(9).fill(null));
    this.solve(board);
    return board as number[][];
  }

  // Remove numbers based on difficulty
  public static generatePuzzle(difficulty: 'easy' | 'medium' | 'hard' | 'expert'): { initial: SudokuBoard, solution: number[][] } {
    const solution = this.generateSolvedBoard();
    const initial: SudokuBoard = solution.map(row => [...row]);

    let attempts = 0;
    const targets = {
      easy: 35, // Numbers to remove
      medium: 45,
      hard: 52,
      expert: 58
    };
    
    const count = targets[difficulty];
    while (attempts < count) {
      const row = Math.floor(Math.random() * 9);
      const col = Math.floor(Math.random() * 9);
      if (initial[row][col] !== null) {
        initial[row][col] = null;
        attempts++;
      }
    }

    return { initial, solution };
  }
}

// src/components/Games/Wordle/WordList.ts

export const WORDS = [
  "ABUSE", "ADULT", "AGENT", "ANGER", "APPLE", "AWARD", "BASIS", "BEACH", "BIRTH", "BLOCK",
  "BOARD", "BRAIN", "BREAD", "BREAK", "BROWN", "BUYER", "CAUSE", "CHAIN", "CHAIR", "CHEST",
  "CHIEF", "CHILD", "CHINA", "CLAIM", "CLASS", "CLOCK", "COACH", "COAST", "COURT", "COVER",
  "CREAM", "CRIME", "CROSS", "CROWD", "CROWN", "CYCLE", "DANCE", "DEATH", "DEPTH", "DIRTY",
  "DRESS", "DRINK", "DRIVE", "EARTH", "ENEMY", "ENTRY", "ERROR", "EVENT", "FAITH", "FAULT",
  "FIELD", "FIGHT", "FINAL", "FLOOR", "FOCUS", "FORCE", "FRAME", "FRANK", "FRONT", "FRUIT",
  "GLASS", "GRANT", "GRASS", "GREEN", "GROUP", "GUIDE", "HEART", "HENRY", "HORSE", "HOTEL",
  "HOUSE", "IMAGE", "INDEX", "INPUT", "ISSUE", "JAPAN", "JONES", "JUDGE", "KNIFE", "LAURA",
  "LAYER", "LEVEL", "LIGHT", "LIMIT", "LUNCH", "MAGIC", "MARCH", "MATCH", "METAL", "MODEL",
  "MONEY", "MONTH", "MOTOR", "MOUTH", "MUSIC", "NIGHT", "NOISE", "NORTH", "NOVEL", "NURSE",
  "OFFER", "ORDER", "OTHER", "OWNER", "PANEL", "PAPER", "PARTY", "PEACE", "PETER", "PHASE",
  "PHONE", "PIANO", "PILOT", "PITCH", "PLACE", "PLANE", "PLANT", "PLATE", "POINT", "POUND",
  "POWER", "PRICE", "PRIDE", "PRIZE", "PROOF", "QUEEN", "RADIO", "RANGE", "RATIO", "REPLY",
  "RIGHT", "RIVER", "ROUND", "ROUTE", "RUGBY", "SCALE", "SCENE", "SCOPE", "SCORE", "SENSE",
  "SHAPE", "SHARE", "SHEEP", "SHEET", "SHIFT", "SHIRT", "SHOCK", "SIGHT", "SIMON", "SKILL",
  "SLEEP", "SMILE", "SMITH", "SMOKE", "SOUND", "SOUTH", "SPACE", "SPEED", "SPITE", "SPORT",
  "SQUAD", "STAFF", "STAGE", "STATE", "STEAM", "STEEL", "STOCK", "STONE", "STORE", "STUDY",
  "STUFF", "STYLE", "SUGAR", "TABLE", "TASTE", "TERRY", "THEME", "THING", "TITLE", "TOTAL",
  "TOUCH", "TOWER", "TRACK", "TRADE", "TRAIN", "TREND", "TRIAL", "TRUST", "TRUTH", "UNCLE",
  "UNION", "UNITY", "VALUE", "VIDEO", "VISIT", "VOICE", "WASTE", "WATCH", "WATER", "WHILE",
  "WHITE", "WHOLE", "WOMAN", "WORLD", "YOUTH",
  // Tech/Geeky words
  "ASYNC", "AWAIT", "LOGIC", "CODER", "ARRAY", "DEBUG", "STACK", "QUEUE", "PROXY", "PATCH",
  "SHELL", "BUILD", "FETCH", "PARSE", "CLOUD", "REACT", "SWIFT", "LINUX", "ERROR", "CACHE",
  "TOKEN", "ADMIN", "LOGIN", "RESET", "CLICK", "EVENT", "WRITE", "QUERY", "MEDIA", "SHARE",
  "STORE", "THEME", "TOOLS", "FRAME", "FILES", "LOCAL", "SERVE", "MOUNT", "FLAGS", "PIXEL",
  "CRASH", "SOLID", "CLEAN", "DRIVE"
];

export const getRandomWord = () => WORDS[Math.floor(Math.random() * WORDS.length)];

export const getDailyWord = () => {
  const epoch = new Date(2024, 0, 1).getTime(); // Jan 1, 2024
  const now = new Date().getTime();
  const dayIndex = Math.floor((now - epoch) / (1000 * 60 * 60 * 24));
  return WORDS[dayIndex % WORDS.length];
};

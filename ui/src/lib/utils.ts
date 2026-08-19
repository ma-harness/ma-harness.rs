import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';

// 2026-08-19 (Day 101 / P7-1.1): cn 工具 — Tailwind class 合并
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

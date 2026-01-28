<?php
enum Status: string {
    case Open = "open";
    public static function tryFrom(string $value): ?self
    {
        return null;
    }
}
                

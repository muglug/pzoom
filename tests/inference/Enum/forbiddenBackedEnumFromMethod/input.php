<?php
enum Status: string {
    case Open = "open";
    public static function from(string $value): self
    {
        throw new Exception;
    }
}
                

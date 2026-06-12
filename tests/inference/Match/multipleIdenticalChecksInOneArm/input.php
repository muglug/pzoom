<?php
function foo(?string $t1, string $t2): string
{
    return match ($t1 ?? $t2) {
        "type1", "type2", "type3" => "1 or 2 or 3",
        "type4", "type5", "type6" => "4 or 5 or 6",
        default => "rest",
    };
}

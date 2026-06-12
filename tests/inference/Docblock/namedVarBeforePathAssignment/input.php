<?php

class Memo {
    /** @var array<string, list<string>> */
    private static array $memoized = [];

    /** @return list<string> */
    public static function tokenize(string $s): array
    {
        $tokens = [];
        foreach (str_split($s) as $i => $c) {
            $tokens[$i] = $c;
        }

        /** @var list<string> $tokens */
        self::$memoized[$s] = $tokens;

        return $tokens;
    }
}

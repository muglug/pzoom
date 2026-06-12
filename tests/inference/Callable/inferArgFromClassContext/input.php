<?php
final class Calc
{
    /**
     * @param Closure(int, int): int $_fn
     */
    public function __invoke(Closure $_fn): int
    {
        return $_fn(42, 42);
    }
}

$calc = new Calc();

$a = $calc(fn($a, $b) => $a + $b);

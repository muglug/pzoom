<?php
final class Calc
{
    /**
     * @param Closure(int, int): int ...$_fn
     */
    public function __invoke(Closure ...$_fn): int
    {
        throw new RuntimeException("???");
    }
}

$calc = new Calc();

$a = $calc(
    foo: fn($_a, $_b) => $_a + $_b,
    bar: fn($_a, $_b) => $_a + $_b,
    baz: fn($_a, $_b) => $_a + $_b,
);

<?php
/**
 * @psalm-type TRelAlternate =
 * list<
 *      array{
 *          href: string,
 *          lang: string
 *      }
 * >
 */
class A {
    /** @return TRelAlternate */
    public function ret(): array { return []; }
}

$_ = (new A)->ret();

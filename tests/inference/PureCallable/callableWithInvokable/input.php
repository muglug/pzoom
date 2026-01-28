<?php
/**
 * @psalm-pure
 */
function asd(): void {}
class A {
    /**
     * @psalm-pure
     */
    public function __invoke(): void {}
}

/**
 * @param pure-callable $p
 */
function fails($p): void {}

fails(new A());
fails("asd");

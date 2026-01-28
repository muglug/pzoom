<?php
/**
 * @psalm-pure
 */
function asd(): void {}
class A {public function __invoke(): void {} }

/**
 * @param pure-callable|A $p
 */
function fails($p): void {}

fails("asd");

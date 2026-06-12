<?php
function asd(): void {}
class A { public function __invoke(): void {} }

/**
 * @param callable|A $p
 */
function fails($p): void {}

fails("asd");

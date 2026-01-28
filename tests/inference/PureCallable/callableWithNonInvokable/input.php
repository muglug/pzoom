<?php
/**
 * @psalm-pure
 */
function asd(): void {}
class B {}

/**
 * @param pure-callable|B $p
 */
function passes($p): void {}

passes("asd");

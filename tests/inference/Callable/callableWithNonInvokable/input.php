<?php
function asd(): void {}
class B {}

/**
 * @param callable|B $p
 */
function passes($p): void {}

passes("asd");

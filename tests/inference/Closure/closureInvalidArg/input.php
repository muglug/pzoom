<?php
/** @param Closure(int): string $c */
function takesClosure(Closure $c): void {}

takesClosure(5);

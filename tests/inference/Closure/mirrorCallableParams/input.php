<?php
namespace NS;
use Closure;
/** @param Closure(int):bool $c */
function acceptsIntToBool(Closure $c): void {}

acceptsIntToBool(Closure::fromCallable(function(int $n): bool { return $n > 0; }));

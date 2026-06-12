<?php
namespace NS;
use Closure;

/** @param Closure(int):bool $c */
function acceptsIntToBool(Closure $c): void {}

class NamedInvokable {
    public function __invoke(int $p): bool {
        return $p > 0;
    }
}

acceptsIntToBool(Closure::fromCallable(new NamedInvokable));

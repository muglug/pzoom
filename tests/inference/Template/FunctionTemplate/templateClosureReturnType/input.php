<?php
namespace NS;

/**
 * @template T
 * @psalm-param \Closure():T $action
 * @psalm-return T
 */
function retry(int $maxRetries, callable $action) {
    return $action();
}

function takesInt(int $p): void{};

takesInt(retry(1, function(): int { return 1; }));
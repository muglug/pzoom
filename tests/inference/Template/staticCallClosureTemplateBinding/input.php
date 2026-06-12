<?php
final class Runner {
    /**
     * @template T
     * @param callable():T $f
     * @return T
     */
    public static function run(callable $f) {
        try {
            return $f();
        } finally {
            error_reporting();
        }
    }
}
function compute(): string {
    return "x";
}
function g(): bool {
    $r = Runner::run(function (): ?string {
        try {
            return compute();
        } catch (Throwable $_e) {
            return null;
        }
    });

    if (null === $r) {
        return false;
    }

    return $r !== '';
}

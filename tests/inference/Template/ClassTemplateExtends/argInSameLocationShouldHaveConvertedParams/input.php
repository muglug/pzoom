<?php
/**
 * @template T
 */
interface I {
    /**
     * @param T $argument
     */
    public function i($argument): void;
}

/**
 * @implements I<int>
 */
class X implements I {
    public function i($argument): void {
        echo sprintf("%d", $argument);
    }
}

/**
 * @implements I<int>
 */
class XWithChangedArgumentName implements I {
    /** @psalm-suppress ParamNameMismatch */
    public function i($changedArgumentName): void {
        echo sprintf("%d", $changedArgumentName);
    }
}
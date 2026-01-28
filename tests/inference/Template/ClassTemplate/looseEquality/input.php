<?php

/**
 * @psalm-immutable
 * @template T of self::READ_UNCOMMITTED|self::READ_COMMITTED|self::REPEATABLE_READ|self::SERIALIZABLE
 */
final class TransactionIsolationLevel
{
    private const READ_UNCOMMITTED = "read uncommitted";
    private const READ_COMMITTED = "read committed";
    private const REPEATABLE_READ = "repeatable read";
    private const SERIALIZABLE = "serializable";

    /**
     * @psalm-var T $level
     */
    private string $level;

    /**
     * @psalm-param T $level
     */
    private function __construct(string $level)
    {
        $this->level = $level;
    }

    /**
     * @psalm-return self<self::READ_UNCOMMITTED>
     */
    public static function readUncommitted(): self
    {
        return new self(self::READ_UNCOMMITTED);
    }

    /**
     * @psalm-return T
     */
    public function toString(): string
    {
        return $this->level;
    }

    /**
     * @psalm-template TResult
     * @psalm-param pure-callable(self::READ_UNCOMMITTED): TResult $readUncommitted
     * @psalm-return TResult
     */
    public function resolve(callable $readUncommitted) {
        if ($this->level == self::READ_UNCOMMITTED) {
            return $readUncommitted($this->level);
        }

        throw new \LogicException("bad");
    }
}
<?php
/**
 * @template T of (self::READ_UNCOMMITTED|self::READ_COMMITTED|self::REPEATABLE_READ|self::SERIALIZABLE)
 */
final class TransactionIsolationLevel {
    private const READ_UNCOMMITTED = 'read uncommitted';
    private const READ_COMMITTED = 'read committed';
    private const REPEATABLE_READ = 'repeatable read';
    private const SERIALIZABLE = 'serializable';

    /**
     * @psalm-var T
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
     * @psalm-return self<self::READ_COMMITTED>
     */
    public static function readCommitted(): self
    {
        return new self(self::READ_COMMITTED);
    }

    /**
     * @psalm-return self<self::REPEATABLE_READ>
     */
    public static function repeatableRead(): self
    {
        return new self(self::REPEATABLE_READ);
    }

    /**
     * @psalm-return self<self::SERIALIZABLE>
     */
    public static function serializable(): self
    {
        return new self(self::SERIALIZABLE);
    }

    /**
     * @psalm-return T
     */
    public function toString(): string
    {
        return $this->level;
    }
}

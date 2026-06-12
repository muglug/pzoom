<?php
namespace Bar;

/**
 * @psalm-type Type = self::NULL|self::BOOL|self::INT|self::STRING
 */
interface I
{
    public const NULL = 0;
    public const BOOL = 1;
    public const INT = 2;
    public const STRING = 3;

    /**
     * @psalm-param Type $type
     */
    public function a(int $type): void;
}

/**
 * @psalm-import-type Type from I as Type2
 */
abstract class C implements I
{
    public function a(int $type): void
    {
        $this->b($type);
    }

    /**
     * @psalm-param Type2 $type
     */
    private function b(int $type): void
    {
    }
}

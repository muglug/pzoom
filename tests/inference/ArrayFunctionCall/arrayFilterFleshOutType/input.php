<?php
class Baz {
    public const STATUS_FOO = "foo";
    public const STATUS_BAR = "bar";
    public const STATUS_QUX = "qux";

    /**
     * @psalm-param self::STATUS_* $role
     */
    public static function isStatus(string $role): bool
    {
        return !\in_array($role, [self::STATUS_BAR, self::STATUS_QUX], true);
    }
}

/** @psalm-var array<Baz::STATUS_*> $statusList */
$statusList = [Baz::STATUS_FOO, Baz::STATUS_QUX];
$statusList = array_filter($statusList, [Baz::class, "isStatus"]);

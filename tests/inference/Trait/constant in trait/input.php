<?php
trait TraitA {
    public const PUBLIC_CONST = 'PUBLIC_CONST';
    protected const PROTECTED_CONST = 'PROTECTED_CONST';
    private const PRIVATE_CONST = 'PRIVATE_CONST';
}
class ClassB {
    use TraitA;
    public static function getPublicConst(): string { return self::PUBLIC_CONST; }
    public static function getProtectedConst(): string { return self::PROTECTED_CONST; }
    public static function getPrivateConst(): string { return self::PRIVATE_CONST; }
}
class ClassC extends ClassB {
    public static function getPublicConst(): string { return self::PUBLIC_CONST; }
    public static function getProtectedConst(): string { return self::PROTECTED_CONST; }
}

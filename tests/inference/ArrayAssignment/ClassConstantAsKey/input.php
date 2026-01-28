<?php
/**
 * @property Foo::C_* $aprop
 */
class Foo {
    public const C_ONE = 1;
    public const C_TWO = 2;

    public function __get(string $prop) {
        if ($prop === "aprop")
            return self::C_ONE;
        throw new \RuntimeException("Unsupported property: $prop");
    }

    /** @return array<Foo::C_*, string> */
    public static function getNames(): array {
        return [
            self::C_ONE => "One",
            self::C_TWO => "Two",
        ];
    }

    public function getThisName(): string {
        $names = self::getNames();
        $aprop = $this->aprop;

        return $names[$aprop];
    }
}

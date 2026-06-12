<?php
abstract class Assert2 {
    /**
     * @psalm-assert !empty $actual
     */
    public static function assertNotEmpty(mixed $actual, string $message = ''): void {}
    /**
     * @template T2
     * @param class-string<T2> $expected
     * @psalm-assert T2 $actual
     */
    public static function assertInstanceOf(string $expected, mixed $actual): void {}
}
final class TLiteral { public string $value = ''; }
abstract class TC2 extends Assert2 {}

final class MyTest extends TC2 {
    /** @param list<TLiteral>|null $resolved */
    public function run(?array $resolved): void {
        self::assertNotEmpty($resolved);
        foreach ($resolved as $type) {
            self::assertInstanceOf(TLiteral::class, $type);
            echo $type->value;
        }
    }
}

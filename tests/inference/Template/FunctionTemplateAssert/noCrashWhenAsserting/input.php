<?php
/**
 * @psalm-template ExpectedClassType of object
 * @psalm-param class-string<ExpectedClassType> $expectedType
 * @psalm-assert class-string<ExpectedClassType> $actualType
 */
function assertIsA(string $expectedType, string $actualType): void {
    \assert(\is_a($actualType, $expectedType, true));
}

class Foo {
    /**
     * @psalm-template OriginalClass of object
     * @psalm-param class-string<OriginalClass> $originalClass
     * @psalm-return class-string<OriginalClass>|null
     */
    private function generateProxy(string $originalClass) : ?string {
        $generatedClassName = self::class . '\\' . $originalClass;

        if (class_exists($generatedClassName)) {
            assertIsA($originalClass, $generatedClassName);

            return $generatedClassName;
        }

        return null;
    }
}
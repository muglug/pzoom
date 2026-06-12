<?php
namespace Psalm\Tests4;
class TypeParseTest4 {
    public function testReflection(): void {
        if (!function_exists('Psalm\Tests4\someFunction4')) {
            /** @psalm-suppress UnusedParam */
            function someFunction4(string $param): string {
                return 'hello';
            }
        }
        $reflectionFunc = new \ReflectionFunction('Psalm\Tests4\someFunction4');
        echo $reflectionFunc->getName();
    }
}

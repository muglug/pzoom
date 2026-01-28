<?php
/**
 * @param class-string<object&callable():int> $className
 */
function takesCallableObject(string $className): int {
    $object = new $className();
    return $object();
}

class Foo
{
    public function __invoke(): int
    {
        return 0;
    }
}

takesCallableObject(Foo::class);
                    

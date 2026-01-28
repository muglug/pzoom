<?php
/**
 * @param class-string<callable-object> $className
 */
function takesCallableObject(string $className): void {
    $object = new $className();
    $object();
}

class Foo
{
    public function __invoke(): void
    {
    }
}

takesCallableObject(Foo::class);
                    

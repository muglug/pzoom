<?php
/**
 * @psalm-template RealObjectType of object
 *
 * @psalm-param class-string<RealObjectType> $className
 * @psalm-param Closure(
 *   RealObjectType|null
 * ) : void $initializer
 */
function createProxy(
    string $className,
    Closure $initializer
) : void {}

class Foo {}

createProxy(Foo::class, function (?Foo $f) : void {});
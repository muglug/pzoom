<?php
/** @psalm-type CallableType = callable */
class Bar
{

}

/** @psalm-import-type CallableType from Bar */
class Foo
{
    /**
     * @param class-string<object&CallableType> $className
     */
    function takesCallableObject(string $className): void {}
}

<?php
/**
 * @template TClass as object
 *
 * @param class-string<TClass> $className
 * @param TClass $realInstance
 *
 * @return Closure(TClass) : void
 * @psalm-suppress InvalidReturnType
 */
function createInitializer(string $className, object $realInstance) : Closure {}

function foo(object $realInstance) : void {
    $className = get_class($realInstance);
    /** @psalm-trace $i */
    $i = createInitializer($className, $realInstance);
}

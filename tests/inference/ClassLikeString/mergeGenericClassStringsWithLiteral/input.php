<?php
class Base {}
class A extends Base {}
class B extends Base {}

/**
 * @param array<A::class|B::class> $literal_classes
 * @param array<class-string<Base>> $generic_classes
 * @return array<class-string<Base>>
 */
function bar(array $literal_classes, array $generic_classes) {
    return array_merge($generic_classes, $literal_classes);
}

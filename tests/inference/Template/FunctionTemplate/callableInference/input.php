<?php
class Foo {}
class FooChild extends Foo {}
class Bar {}

/**
 * @psalm-template ThingType of Foo
 * @psalm-param ThingType $t
 * @return ThingType|Bar
 */
function from_other(Foo $t) {
    if  (rand(0, 1)) {
        return new Bar();
    }
    return $t;
}

/**
 * @param list<FooChild> $a
 * @return list<Bar|FooChild>
 */
function baz(array $a) {
    return array_map("from_other", $a);
}
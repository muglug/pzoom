<?php
class Foo {}
class Bar extends Foo {}
class Baz extends Bar {}

/** @template-covariant T */
class Pair
{
    /** @var T|null */
    public $a;

    /** @var T|null */
    public $b;
}

/** @extends Pair<Foo> */
class FooPair extends Pair
{
    /** @var Bar|null */
    public $a;

    /** @var Baz|null */
    public $b;
}

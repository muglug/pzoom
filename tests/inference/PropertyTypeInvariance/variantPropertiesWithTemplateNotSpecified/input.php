<?php
class Foo {}

/** @template T */
class Pair
{
    /** @var T|null */
    protected $a;

    /** @var T|null */
    protected $b;
}

/**
 * @psalm-suppress MissingTemplateParam
 */
class FooPair extends Pair
{
    /** @var Foo|null */ // Template defaults to mixed, this is invariant
    protected $a;

    /** @var Foo|null */
    protected $b;
}

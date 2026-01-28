<?php
/**
 * @template TKey as array-key
 * @template TValue
 * @template-extends ArrayIterator<TKey, TValue>
 */
class Collection1 extends ArrayIterator{}

/**
 * @psalm-suppress MissingTemplateParam
 */
class Collection2 extends Collection1{}

class Collection3 extends Collection2{}

foreach ((new Collection1(["a" => "b"])) as $a) {}

/** @psalm-suppress MixedAssignment */
foreach ((new Collection2(["a" => "b"])) as $a) {}

/** @psalm-suppress MixedAssignment */
foreach ((new Collection3(["a" => "b"])) as $a) {}

foreach ((new Collection1([])) as $i) {}

/** @psalm-suppress MixedAssignment */
foreach ((new Collection2([])) as $i) {}

/** @psalm-suppress MixedAssignment */
foreach ((new Collection3([])) as $i) {}

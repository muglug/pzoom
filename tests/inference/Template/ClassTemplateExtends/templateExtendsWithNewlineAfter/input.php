<?php
namespace Ns;

/**
 * @template DATA as array<string, scalar|array|object|null>
 */
abstract class Foo {}

/**
 * @template-extends Foo<array{id:int}>
 *
 * @internal
 */
class Bar extends Foo {}
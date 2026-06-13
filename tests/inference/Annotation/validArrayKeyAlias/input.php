<?php
/**
 * @psalm-type ArrayKeyType array-key
 */
class Bar {}

/**
 * @psalm-import-type ArrayKeyType from Bar
 * @psalm-type UsesArrayKeyType array<ArrayKeyType, bool>
 */
class Foo {}

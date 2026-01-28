<?php
/**
 * @psalm-suppress UndefinedDocblockClass
 *
 * @param DoesNotExist<int> $baz
 */
function foobar(DoesNotExist $baz): void {}

/**
 * @psalm-suppress UndefinedDocblockClass, UndefinedClass
 * @var DoesNotExist
 */
$baz = new DoesNotExist();
foobar($baz);

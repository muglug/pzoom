<?php
final class WhateverAssert
{
    /**
     * @param mixed $value
     * @psalm-assert non-empty-list $value
     */
    public static function doAssert($value): void
    {}
}

/** @var iterable<mixed,string> $iterable */
$iterable = [];

WhateverAssert::doAssert($iterable);

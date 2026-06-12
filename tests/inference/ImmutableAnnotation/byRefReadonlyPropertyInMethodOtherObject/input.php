<?php

/** @psalm-immutable */
class Im {
    /** @var list<string> */
    public array $items = [];
}

final class Wrap
{
    public function zz(Im $t): ?string
    {
        return array_shift($t->items);
    }
}

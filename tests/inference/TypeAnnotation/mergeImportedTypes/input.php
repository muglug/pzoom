<?php
namespace A\B;

/**
 * @psalm-type _A=array{
 *      id:int
 * }
 *
 * @psalm-type _B=array{
 *      id:int,
 *      something:int
 * }
 */
class Types
{
}

namespace A;

/**
 * @psalm-import-type _A from \A\B\Types as _AA
 * @psalm-import-type _B from \A\B\Types as _BB
 */
class Id
{
    /**
     * @psalm-param _AA|_BB $_item
     */
    public function ff(array $_item): int
    {
        return $_item["something"];
    }
}

<?php
// Known pzoom-vs-Psalm divergence (return-type prototype).
//
// A trait method returns an array element typed as the base class
// (`non-empty-array<string, AtomicLike>['array']` infers `AtomicLike`) against a
// docblock of subtypes (`@return TArrLike|TKeyedLike`). pzoom reports this
// per-statement as LessSpecificReturnStatement.
//
// Real Psalm reports the same looseness one layer up as MoreSpecificReturnType
// (function-level) because it guards the per-`return` checks inside trait
// bodies; under the return-type prototype (which drops MoreSpecificReturnType)
// Psalm is silent. Mirrors src/Psalm/Type/UnionTrait.php::getArray.
abstract class AtomicLike {}
final class TArrLike extends AtomicLike {}
final class TKeyedLike extends AtomicLike {}

trait ArrayHolderTrait {
    /** @var non-empty-array<string, AtomicLike> */
    protected array $types;

    /** @return TArrLike|TKeyedLike */
    public function getArray(): AtomicLike {
        return $this->types['array'];
    }
}

final class Holder {
    use ArrayHolderTrait;

    /** @param non-empty-array<string, AtomicLike> $types */
    public function __construct(array $types)
    {
        $this->types = $types;
    }
}

<?php
abstract class Atomic2 {}
final class TLit extends Atomic2 { public string $value = ''; }

trait UnionTrait2 {
    /** @return array<array-key, Atomic2> */
    public function getAtomicTypes(): array { return $this->types; }

    /**
     * @psalm-mutation-free
     * @psalm-assert-if-true array<array-key, TLit> $this->getAtomicTypes()
     */
    public function allSpecificLiterals(): bool {
        foreach ($this->types as $t) { if (!$t instanceof TLit) { return false; } }
        return true;
    }
}

final class Union2 {
    use UnionTrait2;

    /** @var array<array-key, Atomic2> */
    private array $types = [];
}

function f(Union2 $left): void {
    if ($left->allSpecificLiterals()) {
        foreach ($left->getAtomicTypes() as $part) {
            echo $part->value;
        }
    }
}

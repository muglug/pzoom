<?php
abstract class Atomic2 {}
final class TLit extends Atomic2 { public string $value = ''; }

final class Union2 {
    /** @var list<Atomic2> */
    private array $types = [];

    /** @return list<Atomic2> */
    public function getAtomicTypes(): array { return $this->types; }

    /**
     * @psalm-mutation-free
     * @psalm-assert-if-true list<TLit> $this->getAtomicTypes()
     */
    public function allSpecificLiterals(): bool {
        foreach ($this->types as $t) { if (!$t instanceof TLit) { return false; } }
        return true;
    }
}

function f(Union2 $left): void {
    if ($left->allSpecificLiterals()) {
        foreach ($left->getAtomicTypes() as $part) {
            echo $part->value;
        }
    }
}

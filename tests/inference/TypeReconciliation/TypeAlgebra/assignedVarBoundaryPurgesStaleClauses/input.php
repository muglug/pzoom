<?php

class InnerUnion {
    /** @psalm-mutation-free */
    public function isNever(): bool { return false; }
    /** @psalm-mutation-free */
    public function isMixed(): bool { return true; }
    /** @psalm-mutation-free */
    public function allStringLiterals(): bool { return false; }
    /** @psalm-mutation-free */
    public function setPossiblyUndefined(bool $flag): self { return $this; }
}

class Combination {
    /** @var list<InnerUnion> */
    public array $array_type_params = [];
    public ?InnerUnion $objectlike_value_type = null;
    /** @var array<string, InnerUnion> */
    public array $objectlike_entries = [];
    public bool $array_always_filled = false;
}

function handleEntries(Combination $c, bool $overwrite): void {
    if ($c->array_type_params
        && $c->array_type_params[0]->allStringLiterals()
        && $c->array_always_filled
    ) {
        $c->array_type_params = [];
    }

    if (!$c->array_type_params || $c->array_type_params[1]->isNever()) {
        if (!$overwrite && $c->array_type_params) {
            foreach ($c->objectlike_entries as &$objectlike_entry) {
                $objectlike_entry = $objectlike_entry->setPossiblyUndefined(true);
            }
            unset($objectlike_entry);
        }

        if ($c->objectlike_value_type
            && $c->objectlike_value_type->isMixed()
            && $c->array_type_params
            && !$c->array_type_params[1]->isNever()
        ) {
            echo 'x';
        }
    }
}

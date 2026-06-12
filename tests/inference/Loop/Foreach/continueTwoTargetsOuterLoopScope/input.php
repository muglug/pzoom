<?php

final class Clause5 {
    public string $hash = 'h';
    /** @var array<string, string> */
    public array $possibilities = [];

    public function removeKey(string $k): ?Clause5
    {
        $p = $this->possibilities;
        unset($p[$k]);
        if ($p === []) {
            return null;
        }
        $c = new self();
        $c->possibilities = $p;
        return $c;
    }
}

/** @param list<Clause5> $clauses
 *  @return list<Clause5> */
function simplify(array $clauses): array
{
    $cloned_clauses = [];
    foreach ($clauses as $clause) {
        $cloned_clauses[$clause->hash] = $clause;
    }

    foreach ($cloned_clauses as $clause_a_hash => $clause_a) {
        $clause_a_keys = array_keys($clause_a->possibilities);

        if (count($clause_a->possibilities) !== 1) {
            foreach ($cloned_clauses as $clause_b) {
                if ($clause_a === $clause_b) {
                    continue;
                }

                if ($clause_a_keys === array_keys($clause_b->possibilities)) {
                    $opposing_keys = [];

                    foreach ($clause_a->possibilities as $key => $a_possibilities) {
                        if ($a_possibilities === 'x') {
                            $opposing_keys[] = $key;
                            continue;
                        }
                        continue 2;
                    }

                    if (count($opposing_keys) === 1) {
                        unset($cloned_clauses[$clause_a_hash]);
                        $clause_a = $clause_a->removeKey($opposing_keys[0]);
                        if (!$clause_a) {
                            continue 2;
                        }
                        $cloned_clauses[$clause_a->hash] = $clause_a;
                    }
                }
            }
        }
    }

    return array_values($cloned_clauses);
}

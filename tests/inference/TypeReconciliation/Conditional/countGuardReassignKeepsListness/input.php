<?php
final class Clause4 {}

/** @param list<Clause4> $if_clauses
 *  @return list<Clause4> */
function f(array $if_clauses): array
{
    if (count($if_clauses) > 200) {
        $if_clauses = [];
    }

    $if_clauses = array_map(
        static function (Clause4 $c): Clause4 {
            return $c;
        },
        $if_clauses,
    );

    return $if_clauses;
}

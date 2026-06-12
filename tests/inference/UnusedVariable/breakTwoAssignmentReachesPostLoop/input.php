<?php

/** @param list<list<string>> $rows
 *  @return list<string>|array<never,never> */
function f(array $rows): array
{
    $out = [];

    if ($out === [] && count($rows) === 2) {
        foreach ($rows as $row) {
            foreach ($row as $cell) {
                if ($cell === 'x') {
                    $out = [];
                    break 2;
                }
            }

            $out[] = 'row';
        }
    }

    return $out;
}

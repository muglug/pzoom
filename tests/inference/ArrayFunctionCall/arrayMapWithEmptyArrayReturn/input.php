<?php
/**
 * @param array<int, array<string>> $elements
 * @return list<string>
 */
function resolvePossibleFilePaths($elements) : array
{
    return array_values(
        array_filter(
            array_merge(
                ...array_map(
                    function (array $element) : array {
                        if (rand(0,1) == 1) {
                            return [];
                        }
                        return $element;
                    },
                    $elements
                )
            )
        )
    );
}

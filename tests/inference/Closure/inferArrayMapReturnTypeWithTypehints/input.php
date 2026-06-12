<?php
/**
 * @param array{0:string,1:string}[] $ret
 * @return array{0:string,1:int}[]
 */
function f(array $ret): array
{
    return array_map(
        /**
         * @param array{0:string,1:string} $row
         */
        function (array $row): array {
            return [
                strval($row[0]),
                intval($row[1]),
            ];
        },
        $ret
    );
}

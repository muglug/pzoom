<?php
/**
 * @param list<string> $list
 * @return list<string>
 */
function getList(array $list): array {
    foreach ($list as $key => $value) {
        if (rand(0, 1)) {
            array_pop($list);
        }

        $list[$key] = $value . "!";
    }

    return $list;
}

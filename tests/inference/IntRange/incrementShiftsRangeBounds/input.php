<?php
/**
 * @param list<int> $args
 * @param list<string> $params
 */
function f(array $args, array $params): void {
    if (count($params) > count($args)) {
        for ($i = count($args), $iMax = count($params); $i < $iMax; $i++) {
            echo $params[$i];
        }
    }
}

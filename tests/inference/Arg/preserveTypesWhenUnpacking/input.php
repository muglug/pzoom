<?php
/**
 * @return array<int,array<int,string>>
 */
function getData(): array
{
    return [
        ["a", "b"],
        ["c", "d"]
    ];
}

/**
 * @return array<int,string>
 */
function f1(): array
{
    $data = getData();
    return array_merge($data[0], $data[1]);
}

/**
 * @return array<int,string>
 */
function f2(): array
{
    $data = getData();
    return array_merge(...$data);
}

/**
 * @return array<int,string>
 */
function f3(): array
{
    $data = getData();
    return array_merge([], ...$data);
}

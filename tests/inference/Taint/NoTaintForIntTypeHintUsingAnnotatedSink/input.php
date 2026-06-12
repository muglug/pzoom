<?php // --taint-analysis
function fetch(int $id): string
{
    return query("SELECT * FROM table WHERE id=" . $id);
}
/**
 * @return string
 * @psalm-taint-sink sql $sql
 * @psalm-taint-specialize
 */
function query(string $sql) { return ""; }
$value = $_GET["value"];
$result = fetch($value);

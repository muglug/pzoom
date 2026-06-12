<?php // --taint-analysis
/** @param int $id */
function fetch($id): string
{
    return query("SELECT * FROM table WHERE id=" . $id);
}
/**
 * @return string
 * @psalm-taint-sink sql $sql
 * @psalm-taint-specialize
 */
function query(string $sql) {}
$value = $_GET["value"];
$result = fetch($value);

<?php
/** @return list<array<string,scalar|null|list<scalar|null>>> */
function fetch_named() : array {
    $p = new PDO("sqlite::memory:");
    $sth = $p->prepare("SELECT 1");
    $sth->execute();
    return $sth->fetchAll(PDO::FETCH_NAMED);
}

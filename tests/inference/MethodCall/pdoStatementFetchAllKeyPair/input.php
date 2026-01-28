<?php
/** @return array<array-key,scalar|null> */
function fetch_column() {
    $p = new PDO("sqlite::memory:");
    $sth = $p->prepare("SELECT 1");
    $sth->execute();
    return $sth->fetchAll(PDO::FETCH_KEY_PAIR);
}

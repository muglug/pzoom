<?php
/** @return scalar|null|false */
function fetch_column() {
    $p = new PDO("sqlite::memory:");
    $sth = $p->prepare("SELECT 1");
    $sth->execute();
    return $sth->fetch(PDO::FETCH_COLUMN);
}

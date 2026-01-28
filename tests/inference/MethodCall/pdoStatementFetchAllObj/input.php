<?php
/** @return list<stdClass> */
function fetch_named() : array {
    $p = new PDO("sqlite::memory:");
    $sth = $p->prepare("SELECT 1");
    $sth->execute();
    return $sth->fetchAll(PDO::FETCH_OBJ);
}

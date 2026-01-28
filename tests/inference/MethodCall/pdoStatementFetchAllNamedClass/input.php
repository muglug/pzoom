<?php
class Foo {}

/** @return list<Foo> */
function fetch_class() : array {
    $p = new PDO("sqlite::memory:");
    $sth = $p->prepare("SELECT 1");
    $sth->execute();
    return $sth->fetchAll(PDO::FETCH_CLASS, Foo::class);
}

<?php
class A {
    /** @var ?string */
    public $a;
}
class B extends A {}

$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_EXCEPTION);
$db->setAttribute(PDO::ATTR_DEFAULT_FETCH_MODE, PDO::FETCH_ASSOC);
$stmt = $db->prepare("select \"a\" as a");
$stmt->setFetchMode(PDO::FETCH_CLASS, A::class);
$stmt2 = $db->prepare("select \"a\" as a");
$stmt2->setFetchMode(PDO::FETCH_ASSOC);
$stmt3 = $db->prepare("select \"a\" as a");
$stmt3->setFetchMode(PDO::ATTR_DEFAULT_FETCH_MODE);
$stmt->execute();
$stmt2->execute();
/** @psalm-suppress MixedAssignment */
$a = $stmt->fetch();
$b = $stmt->fetchAll();
$c = $stmt->fetch(PDO::FETCH_CLASS);
$d = $stmt->fetchAll(PDO::FETCH_CLASS);
$e = $stmt->fetchAll(PDO::FETCH_CLASS, B::class);
$f = $stmt->fetch(PDO::FETCH_ASSOC);
$g = $stmt->fetchAll(PDO::FETCH_ASSOC);
$h = $stmt2->fetch();
$i = $stmt2->fetchAll();
$j = $stmt2->fetch(PDO::FETCH_BOTH);
$k = $stmt2->fetchAll(PDO::FETCH_BOTH);
$l = $stmt3->fetch();

<?php
$list = new SplDoublyLinkedList();
$list->add(5, "hello");
$list->add(5, 1);

/** @var SplDoublyLinkedList<string> */
$templated_list = new SplDoublyLinkedList();
$templated_list->add(5, "hello");
$a = $templated_list->bottom();

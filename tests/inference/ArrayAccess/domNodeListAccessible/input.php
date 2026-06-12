<?php
$doc = new DOMDocument();
$doc->loadXML("<node key=\"value\"/>");
$e = $doc->getElementsByTagName("node")[0];

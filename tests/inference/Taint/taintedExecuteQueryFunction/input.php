<?php
$userId = $_GET["user_id"];
$query = "delete from users where user_id = " . $userId;
$link = mysqli_connect("localhost", "my_user", "my_password", "world");
$result = mysqli_execute_query($link, $query);

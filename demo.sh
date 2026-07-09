#!/bin/bash

echo "Creating demo files for duplicate finder..."

# 创建演示目录
mkdir -p demo_files/folder1
mkdir -p demo_files/folder2

# 创建一些大文件用于测试
echo "Creating test files..."

# 创建 150MB 的文件
dd if=/dev/zero of=demo_files/folder1/video1.mp4 bs=1M count=150 2>/dev/null
cp demo_files/folder1/video1.mp4 demo_files/folder2/video1_copy.mp4

# 创建 200MB 的文件
dd if=/dev/zero of=demo_files/folder1/video2.mkv bs=1M count=200 2>/dev/null
cp demo_files/folder1/video2.mkv demo_files/folder2/video2_duplicate.mkv

# 创建一些小文件（会被忽略）
echo "Small file content" > demo_files/folder1/small.txt
echo "Another small file" > demo_files/folder2/small2.txt

echo "Demo files created!"
echo "You can now scan the 'demo_files' directory with the GUI."
echo ""
echo "To start the GUI:"
echo "./target/release/find-dupl-file --gui"
echo ""
echo "Use 'demo_files' as the scan path and 'demo_disk' as the disk ID."
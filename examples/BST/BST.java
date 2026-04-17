package BST;

public class BST<T extends Comparable<T>> {
    private static class Node<T> {
        T value;
        int height;
        Node<T> left;
        Node<T> right;

        Node(T value) {
            this.value = value;
            this.height = 1;
        }
    }

    private Node<T> root;
    private int size;

    public void insert(T value) {
        root = insert(root, value);
    }

    public boolean contains(T value) {
        Node<T> current = root;
        while (current != null) {
            int cmp = value.compareTo(current.value);
            if (cmp == 0) return true;
            current = cmp < 0 ? current.left : current.right;
        }
        return false;
    }

    public void remove(T value) {
        root = remove(root, value);
    }

    public int size() {
        return size;
    }

    public int height() {
        return height(root);
    }

    public String inorder() {
        StringBuilder sb = new StringBuilder();
        inorder(root, sb);
        return sb.toString().trim();
    }

    private Node<T> insert(Node<T> node, T value) {
        if (node == null) {
            size++;
            return new Node<T>(value);
        }
        int cmp = value.compareTo(node.value);
        if (cmp < 0) {
            node.left = insert(node.left, value);
        } else if (cmp > 0) {
            node.right = insert(node.right, value);
        } else {
            return node;
        }
        return rebalance(node);
    }

    private Node<T> remove(Node<T> node, T value) {
        if (node == null) return null;
        int cmp = value.compareTo(node.value);
        if (cmp < 0) {
            node.left = remove(node.left, value);
        } else if (cmp > 0) {
            node.right = remove(node.right, value);
        } else {
            size--;
            if (node.left == null) return node.right;
            if (node.right == null) return node.left;
            Node<T> successor = minNode(node.right);
            node.value = successor.value;
            size++;
            node.right = remove(node.right, successor.value);
        }
        return rebalance(node);
    }

    private Node<T> minNode(Node<T> node) {
        while (node.left != null) node = node.left;
        return node;
    }

    private Node<T> rebalance(Node<T> node) {
        updateHeight(node);
        int balance = balanceFactor(node);
        if (balance > 1) {
            if (balanceFactor(node.left) < 0) {
                node.left = rotateLeft(node.left);
            }
            return rotateRight(node);
        }
        if (balance < -1) {
            if (balanceFactor(node.right) > 0) {
                node.right = rotateRight(node.right);
            }
            return rotateLeft(node);
        }
        return node;
    }

    private Node<T> rotateLeft(Node<T> node) {
        Node<T> pivot = node.right;
        node.right = pivot.left;
        pivot.left = node;
        updateHeight(node);
        updateHeight(pivot);
        return pivot;
    }

    private Node<T> rotateRight(Node<T> node) {
        Node<T> pivot = node.left;
        node.left = pivot.right;
        pivot.right = node;
        updateHeight(node);
        updateHeight(pivot);
        return pivot;
    }

    private void updateHeight(Node<T> node) {
        node.height = 1 + Math.max(height(node.left), height(node.right));
    }

    private int height(Node<T> node) {
        return node == null ? 0 : node.height;
    }

    private int balanceFactor(Node<T> node) {
        return node == null ? 0 : height(node.left) - height(node.right);
    }

    private void inorder(Node<T> node, StringBuilder sb) {
        if (node == null) return;
        inorder(node.left, sb);
        sb.append(node.value);
        sb.append(' ');
        inorder(node.right, sb);
    }

    public static void main(String[] args) {
        BST<Integer> tree = new BST<Integer>();
        int[] values = {10, 20, 30, 40, 50, 25, 5, 15, 35, 45};
        for (int i = 0; i < values.length; i++) {
            tree.insert(Integer.valueOf(values[i]));
        }
        System.out.println("size=" + tree.size());
        System.out.println("height=" + tree.height());
        System.out.println("inorder=" + tree.inorder());
        System.out.println("contains(25)=" + tree.contains(Integer.valueOf(25)));
        System.out.println("contains(99)=" + tree.contains(Integer.valueOf(99)));

        tree.remove(Integer.valueOf(25));
        tree.remove(Integer.valueOf(10));
        System.out.println("after remove 25,10");
        System.out.println("size=" + tree.size());
        System.out.println("height=" + tree.height());
        System.out.println("inorder=" + tree.inorder());
    }
}

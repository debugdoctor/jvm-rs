package demo;

public class Main {
    public static void main(String[] books) {
        if (books.length != 4) {
            int crash = 1 / 0;
        }

        String[] shelf = new String[6];
        int catalogSize = 0;
        int borrowSuccess = 0;
        int returnSuccess = 0;
        int failedRequests = 0;
        int availableCount = 0;
        int index = 0;

        while (index != books.length) {
            if (books[index] != null) {
                shelf[catalogSize] = books[index];
                catalogSize = catalogSize + 1;
            }
            index = index + 1;
        }

        if (shelf[0] != null) {
            shelf[0] = null;
            borrowSuccess = borrowSuccess + 1;
        } else {
            failedRequests = failedRequests + 1;
        }

        if (shelf[0] != null) {
            shelf[0] = null;
            borrowSuccess = borrowSuccess + 1;
        } else {
            failedRequests = failedRequests + 1;
        }

        if (shelf[1] != null) {
            shelf[1] = null;
            borrowSuccess = borrowSuccess + 1;
        } else {
            failedRequests = failedRequests + 1;
        }

        if (shelf[0] == null) {
            shelf[0] = books[0];
            if (books[0] != null) {
                returnSuccess = returnSuccess + 1;
            } else {
                failedRequests = failedRequests + 1;
            }
        } else {
            failedRequests = failedRequests + 1;
        }

        if (books[2] != null) {
            if (shelf[3] == null) {
                shelf[3] = books[2];
                catalogSize = catalogSize + 1;
            } else {
                failedRequests = failedRequests + 1;
            }
        } else {
            failedRequests = failedRequests + 1;
        }

        if (shelf[3] != null) {
            shelf[3] = null;
            borrowSuccess = borrowSuccess + 1;
        } else {
            failedRequests = failedRequests + 1;
        }

        index = 0;
        while (index != shelf.length) {
            if (shelf[index] != null) {
                availableCount = availableCount + 1;
            }
            index = index + 1;
        }

        int checksum = availableCount
            + borrowSuccess * 10
            + returnSuccess * 100
            + failedRequests * 1000
            + catalogSize * 10000;

        if (checksum != 42132) {
            int crash = 1 / 0;
            failedRequests = crash;
        }
    }
}
